//! Reviewed EVP1 production for the independently provable P0 subset.

use std::collections::{HashMap, HashSet};

use evocc_evoir::{inst_results, render_module, validate_module, CmpOp, Inst, Module, Value};
use evocc_proof::{
    sha256_hex, EvpBudget, EvpCertificate, EvpCongruence, EvpEffectClass, EvpEvent, EvpExtraction,
    EvpExtractionChoice, EvpNode, EvpNodeSource, EvpPremise, EvpProofRef, EvpQuantity, EvpRegion,
    EvpRegionId, EvpRewrite, EvpSeed, EvpStop, EvpVersion,
};
use evocc_types::EvoIRType;

use crate::eligibility::Admission;
use crate::known_bits::{Comparison, KnownBits};
use crate::scalar_eq::{
    fold_raw, ingest_scalar_inst, mask_raw, optimize_scalar_module_with_events, remap_inst_uses,
    resolve_alias, scalar_operands, AppliedRewrite, NegationEnv, Replacement, RewriteOpportunity,
    ScalarFact, ScalarOperator,
};
use crate::seed_facts::SeedFacts;
use crate::{ProofAuthority, ScalarEqBudget, ScalarEqReport, ScalarRewrite};

/// The two reviewed registries of Section 11.6.
///
/// A certificate commits to one registry and cannot extend it, and Section 11.6
/// admits either under `evp2`, because the revision includes every rule of the
/// earlier one unchanged and in order. The producer names the narrowest one
/// that contains every rule it actually fired: that is the smaller claim, and
/// it leaves a premise-free certificate readable by a checker that knows only
/// the earlier registry.
const REGISTRY_V1: &str = "p0.scalar.v1";
const REGISTRY_V1_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
const REGISTRY_V2: &str = "p0.scalar.v2";
const REGISTRY_V2_SHA256: &str = "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce";
const REGISTRY_V3: &str = "p0.scalar.v3";
const REGISTRY_V3_SHA256: &str = "aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15";

/// The rules each revision added, which is what makes the narrowest registry a
/// question about names rather than about numbers: `p1_206` is printed by the
/// third registry although it sits between two rules of the second.
const V2_RULES: [&str; 18] = [
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
const V3_RULES: [&str; 6] = [
    "p1_206_known_shl",
    "p1_207_known_lshr",
    "p1_208_known_ashr",
    "p1_230_known_zext",
    "p1_231_known_sext",
    "p1_232_known_trunc",
];
/// Section 8.11: the P0 profile's ceilings. A budget line records these and no
/// producer-local ones; a tighter `ScalarEqBudget` is this producer's own stop
/// rule, not the profile's box, and recording it would make the checker refuse
/// every certificate for disagreeing with the profile.
const MAX_NODES: u64 = 100_000;
const MAX_EPOCHS: u64 = 1_024;
/// The floor of the per-rule fuel bound, `max(256, 8 * n)`.
const FUEL_FLOOR: u64 = 256;

pub(crate) type ValueRemap = HashMap<(String, u32), u32>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedScalarEqOutcome {
    pub module: Module,
    pub certificate: EvpCertificate,
    pub report: ScalarEqReport,
}

pub(crate) struct CertifiedScalarEqTransaction {
    pub(crate) outcome: CertifiedScalarEqOutcome,
    pub(crate) events: Vec<AppliedRewrite>,
    pub(crate) value_remap: ValueRemap,
}

/// Optimize only regions that can carry a self-contained current-transaction
/// EVP1 proof. More speculative or history-dependent opportunities are simply
/// omitted; optimization is never required for correctness.
#[must_use]
pub fn optimize_scalar_module_with_evp1(
    module: &Module,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqOutcome {
    optimize_scalar_module_with_evp1_transaction(module, budget).outcome
}

pub(crate) fn optimize_scalar_module_with_evp1_transaction(
    module: &Module,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqTransaction {
    let input = render_module(module);
    let (transient, applied) = optimize_scalar_module_with_events(module, budget);
    let mut report = transient.report;
    report.receipts.clear();
    report.merges = 0;
    report.optimized_values = 0;
    report.proof_authority = ProofAuthority::Reviewed {
        registry: REGISTRY_V1,
    };
    if report.fell_back {
        return zero_transaction(module, input, report);
    }

    let mut regions = Vec::new();
    let mut accepted = Vec::new();
    let mut events = Vec::new();
    let mut proof_nodes = 0_usize;
    'admit: for (function_index, function) in module.functions.iter().enumerate() {
        // Section 8.5: taint and exclusion are computed once per function,
        // before any region is formed, and consulted from there.
        let admission = Admission::of(function);
        // Section 8.10: what every edge into a block agrees its arguments
        // carry, computed once for the function before any region is formed.
        let seed_facts = SeedFacts::of(function, &admission);
        report.seed_fact_rounds = report.seed_fact_rounds.max(seed_facts.passes());
        for (block_index, block) in function.blocks.iter().enumerate() {
            let context = Surroundings {
                shapes: block_shapes(block),
                admission: &admission,
                seed_facts: &seed_facts,
            };
            for (instruction_index, instruction) in block.insts.iter().enumerate() {
                let Some(root_event) = applied.iter().find(|event| {
                    event.function == function.name
                        && event.block == block.id
                        && inst_results(instruction)
                            .first()
                            .is_some_and(|result| result.id == event.result.id)
                }) else {
                    continue;
                };
                let Some((region, normalized_event, output_replacement, congruent_root)) =
                    build_region(
                        module,
                        function_index,
                        block_index,
                        instruction_index,
                        &context,
                        &applied,
                        root_event,
                    )
                else {
                    continue;
                };
                if congruent_root {
                    report.congruent_roots += 1;
                }
                let new_nodes = region
                    .events
                    .iter()
                    .filter(|event| matches!(event, EvpEvent::Node(_)))
                    .count();
                if regions.len() >= budget.max_epochs
                    || proof_nodes.saturating_add(new_nodes) > budget.max_nodes
                {
                    // Section 11.3: hitting any ceiling keeps every region
                    // admitted before the stop. Answering exhaustion with a
                    // zero-region certificate is non-conforming, because it
                    // discards work a checker would have verified.
                    report.budget_exhausted = true;
                    break 'admit;
                }
                proof_nodes += new_nodes;
                accepted.push(Accepted {
                    function: function_index,
                    block: block_index,
                    instruction: instruction_index,
                    root: normalized_event.result.id,
                    replacement: output_replacement,
                });
                events.push(normalized_event);
                regions.push(region);
            }
        }
    }
    if regions.is_empty() {
        // Section 11.3: a certificate with no region block claims that the
        // output is the canonical rendering of the parsed input and nothing
        // else. Renumbering a module on the way out of a transaction that
        // admitted nothing would make that claim false while the optimizer
        // believed it had done no work.
        return zero_transaction(module, input, report);
    }
    let (output, value_remap) = match apply(module, &accepted) {
        Ok(output) => output,
        Err(()) => {
            report.proof_replay_failed = true;
            report.fell_back = true;
            return zero_transaction(module, input, report);
        }
    };
    let output_text = render_module(&output);
    report.regions_admitted = regions.len();
    report.graph_nodes = proof_nodes;
    report.epochs = regions.len();
    report.merges = regions.len();
    report.optimized_values = regions.len();
    report.congruences = regions
        .iter()
        .flat_map(|region| &region.events)
        .filter(|event| matches!(event, EvpEvent::Congruence(_)))
        .count();
    let (registry_name, registry_digest) = registry_for(&regions);
    report.proof_authority = ProofAuthority::Reviewed {
        registry: registry_name,
    };
    CertifiedScalarEqTransaction {
        outcome: CertifiedScalarEqOutcome {
            module: output,
            certificate: EvpCertificate {
                version: EvpVersion::Evp2,
                registry_name: registry_name.to_owned(),
                registry_digest: registry_digest.to_owned(),
                input_digest: sha256_hex(input.as_bytes()),
                budget: Some(scheduler_record(&regions, report.budget_exhausted)),
                regions,
                output_digest: sha256_hex(output_text.as_bytes()),
            },
            report,
        },
        events,
        value_remap,
    }
}

/// The registry a set of region blocks commits to.
///
/// Every rule the revision adds is printed with a `p1_` identifier and every
/// rule it inherits with a `p0_` one, so the prefix is the whole test.
fn registry_for(regions: &[EvpRegion]) -> (&'static str, &'static str) {
    let mut fired =
        regions
            .iter()
            .flat_map(|region| &region.events)
            .filter_map(|event| match event {
                EvpEvent::Rewrite(rewrite) => Some(rewrite.rule.as_str()),
                _ => None,
            });
    let mut widest = (REGISTRY_V1, REGISTRY_V1_SHA256);
    for rule in &mut fired {
        if V3_RULES.contains(&rule) {
            return (REGISTRY_V3, REGISTRY_V3_SHA256);
        }
        if V2_RULES.contains(&rule) {
            widest = (REGISTRY_V2, REGISTRY_V2_SHA256);
        }
    }
    widest
}

/// The scheduler's record for one completed transaction.
///
/// `n` is the largest node count any admitted region held when the transaction
/// began: the tightest declaration that still bounds every region's per-rule
/// allowance. Section 11.3 permits overstating it, and understating it is what
/// a checker would catch.
fn scheduler_record(regions: &[EvpRegion], exhausted: bool) -> EvpBudget {
    let mut nodes = 0_u64;
    let mut widest = 0_u64;
    let mut epochs = 0_u64;
    // Fuel is per rule per epoch, so a rule that fires in two epochs has not
    // spent twice the fuel of one; the consumption is the busiest single pair.
    let mut firings: HashMap<(&str, u64), u64> = HashMap::new();
    for region in regions {
        let mut before = 0_u64;
        let mut epoch = 0_u64;
        for event in &region.events {
            match event {
                EvpEvent::Node(node) => {
                    nodes += 1;
                    if node.source == EvpNodeSource::Before {
                        before += 1;
                    }
                }
                EvpEvent::Rewrite(rewrite) => {
                    *firings.entry((rewrite.rule.as_str(), epoch)).or_default() += 1;
                }
                EvpEvent::Epoch(marker) => {
                    epoch = *marker;
                    epochs = epochs.max(*marker);
                }
                EvpEvent::Congruence(_) => {}
            }
        }
        widest = widest.max(before);
    }
    EvpBudget {
        nodes: EvpQuantity {
            consumed: nodes,
            bound: MAX_NODES,
        },
        epochs: EvpQuantity {
            consumed: epochs,
            bound: MAX_EPOCHS,
        },
        fuel: EvpQuantity {
            consumed: firings.values().copied().max().unwrap_or(0),
            bound: FUEL_FLOOR.max(widest.saturating_mul(8)),
        },
        n: widest,
        // Section 8.11 as restored at revision 1.9.38: `saturated` means no
        // rule fired in the last epoch, which is why the sweep ended, and says
        // nothing about a fresh transaction over the output. That a fresh one
        // may still admit *residual regions* is a property of the registry --
        // Section 11.6 lets a left-hand `const_like` match only a constant
        // already in the before witness closure -- and is measured beside the
        // campaign rather than recorded here. A stop at this producer's own
        // budget is neither saturation nor a profile ceiling, and `policy` is
        // the reason for exactly that.
        stop: if exhausted {
            EvpStop::Policy
        } else {
            EvpStop::Saturated
        },
    }
}

fn zero_transaction(
    module: &Module,
    input: String,
    report: ScalarEqReport,
) -> CertifiedScalarEqTransaction {
    CertifiedScalarEqTransaction {
        outcome: zero(module, input, report),
        events: Vec::new(),
        value_remap: HashMap::new(),
    }
}

fn zero(module: &Module, input: String, mut report: ScalarEqReport) -> CertifiedScalarEqOutcome {
    report.merges = 0;
    report.optimized_values = 0;
    CertifiedScalarEqOutcome {
        module: module.clone(),
        certificate: EvpCertificate {
            version: EvpVersion::Evp2,
            registry_name: REGISTRY_V1.to_owned(),
            registry_digest: REGISTRY_V1_SHA256.to_owned(),
            input_digest: sha256_hex(input.as_bytes()),
            budget: Some(scheduler_record(&[], report.budget_exhausted)),
            regions: Vec::new(),
            output_digest: sha256_hex(input.as_bytes()),
        },
        report,
    }
}

#[derive(Clone)]
struct Accepted {
    function: usize,
    block: usize,
    instruction: usize,
    root: u32,
    replacement: Replacement,
}

/// What the function and the block around a candidate root already answered.
///
/// The shapes are computed once per block and the admission facts once per
/// function, so a candidate reads them rather than recomputing either.
struct Surroundings<'a> {
    shapes: BlockShapes,
    admission: &'a Admission,
    seed_facts: &'a SeedFacts,
}

fn build_region(
    module: &Module,
    function_index: usize,
    block_index: usize,
    instruction_index: usize,
    surroundings: &Surroundings,
    applied: &[AppliedRewrite],
    root_event: &AppliedRewrite,
) -> Option<(EvpRegion, AppliedRewrite, Replacement, bool)> {
    let function = &module.functions[function_index];
    let block = &function.blocks[block_index];
    let instruction = &block.insts[instruction_index];
    ingest_scalar_inst(instruction).ok()?;
    let definitions = block
        .insts
        .iter()
        .enumerate()
        .flat_map(|(index, instruction)| {
            inst_results(instruction)
                .into_iter()
                .map(move |value| (value.id, index))
        })
        .collect::<HashMap<_, _>>();
    let mut closure = HashSet::new();
    collect_closure(block, &definitions, instruction_index, &mut closure)?;
    join_duplicates(
        block,
        &definitions,
        &surroundings.shapes,
        instruction_index,
        &mut closure,
    )?;
    let witness_indices = (0..block.insts.len())
        .filter(|index| closure.contains(index))
        .collect::<Vec<_>>();
    let mut seed_values = Vec::new();
    let mut seen = HashSet::new();
    for index in &witness_indices {
        for operand in scalar_operands(&block.insts[*index]) {
            let inside = definitions
                .get(&operand.id)
                .is_some_and(|definition| closure.contains(definition));
            if !inside && seen.insert(operand.id) {
                seed_values.push(operand.clone());
            }
        }
    }
    // Section 8.5: a region is ineligible when its root, any witness, or any
    // seed is tainted or excluded. That is the whole condition. Which block
    // the region sits in is not part of it, and neither is whether a seed is
    // an argument of the entry block rather than of the block the region is
    // in: the exclusion fixed point already decides what may be opaque, and
    // narrowing it further loses every region of every later block.
    if !witness_indices
        .iter()
        .flat_map(|index| inst_results(&block.insts[*index]))
        .chain(seed_values.iter())
        .all(|value| surroundings.admission.admits(value.id))
    {
        return None;
    }
    let seeds = seed_values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            Some(EvpSeed {
                value: value.id,
                sort: packed_type(&value.ty)?,
                origin: surroundings.admission.origin(value.id),
                class: u32::try_from(index).ok()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let mut value_classes = seeds
        .iter()
        .map(|seed| (seed.value, seed.class))
        .collect::<HashMap<_, _>>();
    let mut value_nodes = HashMap::new();
    let mut node_values = HashMap::new();
    let mut nodes = Vec::new();
    for index in &witness_indices {
        let instruction = &block.insts[*index];
        let node_id = u32::try_from(nodes.len()).ok()?;
        let owner = u32::try_from(seeds.len() + nodes.len()).ok()?;
        let node = before_node(instruction, node_id, owner, &value_classes)?;
        let result = inst_results(instruction)[0];
        value_classes.insert(result.id, owner);
        value_nodes.insert(result.id, node_id);
        node_values.insert(node_id, result.clone());
        nodes.push(node);
    }
    let root_node = *value_nodes.get(&root_event.result.id)?;
    let replacement_root = nodes.get(root_node as usize)?.owner;
    let mut classes = LocalClasses::new(seeds.len() + nodes.len())?;
    // One width per class, in class order, so a fact can be started at top
    // without consulting a node that may not exist yet.
    let mut widths = seeds
        .iter()
        .map(|seed| sort_width(&seed.sort))
        .chain(nodes.iter().map(|node| sort_width(&node.sort)))
        .collect::<Option<Vec<_>>>()?;
    // Section 8.10 "Facts of seeds": a seed of origin `input` is not opaque to
    // the analysis, only to the region. Its class starts at the fact the
    // function-wide fixed point derived rather than at top, which is what lets
    // a side condition read what a block argument carries. Every other seed
    // starts at top.
    let seed_starts = seeds
        .iter()
        .zip(&widths)
        .map(|(seed, width)| {
            surroundings
                .seed_facts
                .of_value(seed.value)
                .filter(|fact| fact.width() == *width)
                .unwrap_or_else(|| KnownBits::top(*width))
        })
        .collect::<Vec<_>>();
    // The before nodes are the region as the transaction found it, produced by
    // no epoch, so they precede the first marker.
    let mut events = nodes
        .iter()
        .cloned()
        .map(EvpEvent::Node)
        .collect::<Vec<_>>();
    // Which epoch last changed a class, so a rewrite that matches on a class an
    // earlier rewrite merged is placed after it rather than beside it.
    let mut changed: Vec<(u32, u64)> = Vec::new();
    let mut marker = 0_u64;
    // Section 7.4: nodes that are congruent as they are inserted are already one
    // class, and no epoch produced that. Two `const` nodes with the same payload
    // are the whole of this case in the P0 profile, and they are the two classes
    // holding one constant that the first campaign's refusals were reduced to.
    close_congruences(&nodes, &mut classes, &mut events, marker, &mut changed)?;
    let mut selected = None;
    let mut normalized_root = None;
    let mut output_replacement = None;
    // Whether the root instruction is replaced on the strength of a congruence
    // alone, with no rewrite of its own.
    let mut congruent_root = false;
    for event in applied.iter().filter(|event| {
        event.function == function.name
            && event.block == block.id
            && definitions
                .get(&event.result.id)
                .is_some_and(|index| closure.contains(index))
    }) {
        let node_id = *value_nodes.get(&event.result.id)?;
        let lhs = classes.find(nodes.get(node_id as usize)?.owner)?;
        if event.rule == ScalarRewrite::Congruent {
            // Section 7.4 already put this node in its twin's class, and the
            // closure recorded that as a congruence event. There is no rule to
            // state and nothing to merge; what the region claims is that the
            // twin is a cheaper member of the class the root ended in.
            if event.result.id == root_event.result.id {
                let (chosen, replacement) =
                    cheaper_member(&seed_values, &nodes, &node_values, &classes, lhs, node_id)?;
                selected = Some(chosen);
                normalized_root = Some(event.clone());
                output_replacement = Some(replacement);
                congruent_root = true;
            }
            continue;
        }
        // Section 8.10 as the checker will recompute it: from the region's own
        // nodes, after the merges recorded so far. Reading the sweep's
        // environment instead would let the producer state a premise nobody
        // else derives.
        let facts = RegionFacts::compute(&nodes, &widths, &seed_starts, &classes)?;
        let derived = region_rewrite(event, &nodes, &value_classes, &classes, &facts, node_id)?;
        let opportunity = derived.opportunity;
        let rule = derived.rule;
        let normalized_event = AppliedRewrite {
            function: event.function.clone(),
            block: event.block,
            result: event.result.clone(),
            instruction: event.instruction.clone(),
            facts: event.facts.clone(),
            twin: None,
            negations: event.negations.clone(),
            known: event.known.clone(),
            rule: opportunity.rule,
            replacement: opportunity.replacement.clone(),
        };
        let bindings = derived
            .bindings
            .iter()
            .map(|(name, path)| {
                Some((
                    (*name).to_owned(),
                    binding_class(&nodes, &classes, node_id, *path)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        // Section 11.6 fixes which class each side condition read and in what
        // order; the fact recorded is the one this region's nodes give it.
        let premises = derived
            .premises
            .iter()
            .map(|source| {
                let class = match source {
                    PremiseSource::Matched => lhs,
                    PremiseSource::Variable(name) => bindings
                        .iter()
                        .find(|(bound, _)| bound == name)
                        .map(|(_, class)| *class)?,
                };
                let fact = facts.of(class, &classes)?;
                Some(EvpPremise {
                    class,
                    mask: fact.mask(),
                    value: fact.value(),
                    digits: usize::from(fact.width().div_ceil(4)),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        // Section 8.11: an epoch runs the tiers in order because the facts one
        // sharpens are what the next matches on. A rewrite therefore belongs to
        // the epoch after the last one that changed a class it matches on.
        let mut depth = 0_u64;
        let matched = nodes.get(node_id as usize)?.children.clone();
        for class in matched
            .into_iter()
            .chain(bindings.iter().map(|(_, class)| *class))
        {
            let root = classes.find(class)?;
            for (recorded, epoch) in &changed {
                if classes.find(*recorded) == Some(root) {
                    depth = depth.max(*epoch);
                }
            }
        }
        // Markers strictly increase within a block, so a rewrite that was ready
        // earlier is recorded in the epoch the block has reached. Firing a ready
        // rule later is a schedule a scheduler may choose; claiming it fired
        // before the merge it matches on would not be.
        let epoch = (depth + 1).max(marker);
        let (rhs, rhs_ref, event_selected, event_output) = match &opportunity.replacement {
            Replacement::Alias(_) => {
                // Section 11.3: `class:cI` is admissible only when that side of
                // the instantiated rule is a bare variable bound to `cI`, so the
                // right-hand side is the binding and not the instruction's
                // operand. Reading the operand instead lets an earlier
                // replacement in the same witness closure point it at a
                // different class holding an equal constant, which is the defect
                // the bindings themselves already had.
                let named = derived.right_hand_variable?;
                let rhs = bindings
                    .iter()
                    .find(|(name, _)| name == named)
                    .map(|(_, class)| *class)?;
                let (selected, replacement) =
                    alias_representative(&seed_values, &nodes, &node_values, &classes, rhs)?;
                (rhs, EvpProofRef::Class(rhs), selected, replacement)
            }
            Replacement::Constant(raw) => {
                let sort = packed_type(&event.result.ty)?;
                let raw = mask_raw(*raw, &event.result.ty);
                if let Some(existing) = nodes.iter().find(|node| {
                    node.operation == "const"
                        && node.sort == sort
                        && matches!(node.attributes.as_slice(), [(name, value)] if name == "value" && *value == raw)
                }) {
                    let output = node_values
                        .get(&existing.node)
                        .cloned()
                        .map(Replacement::Alias)
                        .unwrap_or(Replacement::Constant(raw));
                    (
                        classes.find(existing.owner)?,
                        EvpProofRef::Node(existing.node),
                        SelectedSource::Node(existing.node),
                        output,
                    )
                } else {
                    let node = u32::try_from(nodes.len()).ok()?;
                    let owner = classes.push()?;
                    widths.push(sort_width(&sort)?);
                    let fresh = EvpNode {
                        node,
                        owner,
                        source: EvpNodeSource::Derived,
                        operation: "const".to_owned(),
                        sort,
                        effect: EvpEffectClass::Pure,
                        attributes: vec![("value".to_owned(), raw)],
                        children: Vec::new(),
                    };
                    nodes.push(fresh.clone());
                    events.push(EvpEvent::Node(fresh));
                    (
                        owner,
                        EvpProofRef::Node(node),
                        SelectedSource::Node(node),
                        Replacement::Constant(raw),
                    )
                }
            }
        };
        // Section 11.3 requires a union's endpoints to be two distinct canonical
        // roots. When congruence has already put them in one class the rewrite
        // proves nothing and is not recorded -- but what it would have proved
        // still holds, so the replacement it names stands and the region keeps
        // it. Dropping the region here instead would make closing congruence a
        // net loss: the merge that established the equality would also be the
        // reason the equality went unused.
        if lhs != rhs {
            let result = lhs.min(rhs);
            if epoch > marker {
                events.push(EvpEvent::Epoch(epoch));
                marker = epoch;
            }
            events.push(EvpEvent::Rewrite(EvpRewrite {
                rule: rule.to_owned(),
                lhs_pre: lhs,
                rhs_pre: rhs,
                result,
                lhs: EvpProofRef::Node(node_id),
                rhs: rhs_ref,
                bindings,
                premises,
            }));
            classes.union(lhs, rhs)?;
            changed.push((result, epoch));
            // Section 8.4 repairs congruence after a merge, so what this merge
            // made congruent belongs to the epoch that made it.
            close_congruences(&nodes, &mut classes, &mut events, epoch, &mut changed)?;
        } else if event.result.id == root_event.result.id {
            // The root's own replacement rests on a congruence rather than on a
            // rule. Counted, because it is the measure of what closing
            // congruence buys.
            congruent_root = true;
        }
        if event.result.id == root_event.result.id {
            selected = Some(event_selected);
            normalized_root = Some(normalized_event);
            output_replacement = Some(event_output);
        }
    }
    let root_class = classes.find(replacement_root)?;
    let extractions = extraction_manifest(
        &nodes,
        &seeds,
        &value_classes,
        selected?,
        replacement_root,
        &classes,
    )?;
    let block_label = if block_index == 0 {
        "entry".to_owned()
    } else {
        format!("b{}", block.id)
    };
    let region_id = |index: usize| EvpRegionId {
        function: function.name.clone(),
        block: block_label.clone(),
        instruction: u64::try_from(index).expect("usize always fits u64 on supported targets"),
    };
    Some((
        EvpRegion {
            id: region_id(instruction_index),
            root_value: inst_results(instruction)[0].id,
            witnesses: witness_indices.into_iter().map(region_id).collect(),
            seeds,
            events,
            extractions,
            root_class,
        },
        normalized_root?,
        output_replacement?,
        congruent_root,
    ))
}

/// Section 7.4: equality is a congruence, so two nodes with the same operation
/// whose children are pairwise in one class are themselves one class. Section
/// 11.3 gives each such merge its own event, because "a format carrying only
/// rewrites cannot reproduce the partition": a reader replaying the stream has
/// to reach the classes the producer reached.
///
/// Nodes are grouped by what makes them the same node and the first of each
/// group absorbs the rest, in node order, and the pass repeats while anything
/// merges. Grouping rather than comparing every pair matters once a closure
/// can hold every duplicate of every constant in its block, which is what
/// Section 8.5 admitted at revision 1.9.44.
///
/// A congruence cannot contradict a fact. Two congruent nodes have the same
/// operation and pairwise equal children, so Section 8.10 transfers them to the
/// same fact, and the meet of a fact with itself is itself. What their classes
/// hold besides those two nodes is checked where every other union is checked,
/// when the facts are next recomputed.
fn close_congruences(
    nodes: &[EvpNode],
    classes: &mut LocalClasses,
    events: &mut Vec<EvpEvent>,
    epoch: u64,
    changed: &mut Vec<(u32, u64)>,
) -> Option<()> {
    type Shape = (String, String, EvpEffectClass, Vec<(String, u64)>, Vec<u32>);
    loop {
        let mut first_of: HashMap<Shape, u32> = HashMap::new();
        let mut merged = false;
        for node in nodes {
            let children = node
                .children
                .iter()
                .map(|child| classes.find(*child))
                .collect::<Option<Vec<_>>>()?;
            let shape = (
                node.operation.clone(),
                node.sort.clone(),
                node.effect,
                node.attributes.clone(),
                children,
            );
            let Some(first) = first_of.get(&shape).copied() else {
                first_of.insert(shape, node.node);
                continue;
            };
            let lhs_pre = classes.find(nodes.get(first as usize)?.owner)?;
            let rhs_pre = classes.find(node.owner)?;
            if lhs_pre == rhs_pre {
                continue;
            }
            let result = lhs_pre.min(rhs_pre);
            events.push(EvpEvent::Congruence(EvpCongruence {
                lhs_pre,
                rhs_pre,
                result,
                lhs_node: first,
                rhs_node: node.node,
            }));
            classes.union(lhs_pre, rhs_pre)?;
            changed.push((result, epoch));
            merged = true;
        }
        if !merged {
            return Some(());
        }
    }
}

struct LocalClasses {
    parent: Vec<u32>,
}

impl LocalClasses {
    fn new(count: usize) -> Option<Self> {
        Some(Self {
            parent: (0..count)
                .map(|class| u32::try_from(class).ok())
                .collect::<Option<Vec<_>>>()?,
        })
    }

    fn push(&mut self) -> Option<u32> {
        let class = u32::try_from(self.parent.len()).ok()?;
        self.parent.push(class);
        Some(class)
    }

    fn find(&self, mut class: u32) -> Option<u32> {
        loop {
            let parent = *self.parent.get(class as usize)?;
            if parent == class {
                return Some(class);
            }
            class = parent;
        }
    }

    fn union(&mut self, left: u32, right: u32) -> Option<()> {
        let left = self.find(left)?;
        let right = self.find(right)?;
        if left == right {
            return None;
        }
        let root = left.min(right);
        let other = left.max(right);
        *self.parent.get_mut(other as usize)? = root;
        Some(())
    }
}

/// Which rule of the reviewed registry a recorded rewrite instantiates in this
/// region, with its bindings and the premises its side condition reads.
struct RegionRewrite {
    opportunity: RewriteOpportunity,
    rule: &'static str,
    bindings: Vec<(&'static str, BindingPath)>,
    /// The bare variable the right-hand side names, or `None` when the
    /// right-hand side is a builder and needs a `node:` reference.
    right_hand_variable: Option<&'static str>,
    premises: Vec<PremiseSource>,
}

/// Which class one premise names, resolved once the bindings are known.
#[derive(Clone, Copy)]
enum PremiseSource {
    /// `fully_known` reads the class of the matched left-hand node.
    Matched,
    /// `covers` reads `?m` then `?x`; `known_zero` reads `?z`.
    Variable(&'static str),
}

/// The rule this region states for one recorded rewrite.
///
/// Section 11.6 as fixed at revision 1.9.41: within a tier the producer applies
/// the first applicable rule in registry order, so a premise-free fold on a
/// node precedes a fact-driven rule on the same node. That is what makes
/// `p1_202`, `p1_203` and `p1_204` dominated: `fully_known` on `xor`, `add` and
/// `sub` holds only when both operands are fully known, which is exactly when
/// `all_const` holds.
fn region_rewrite(
    event: &AppliedRewrite,
    nodes: &[EvpNode],
    value_classes: &HashMap<u32, u32>,
    classes: &LocalClasses,
    facts: &RegionFacts,
    node_id: u32,
) -> Option<RegionRewrite> {
    let matched = nodes.get(node_id as usize)?;
    if let Some(opportunity) = canonical_reduction(event, matched, nodes, value_classes, classes) {
        if let Some((rule, bindings)) = exact_rule(
            &event.instruction,
            &event.facts,
            &event.negations,
            &opportunity,
        ) {
            let right_hand_variable =
                matches!(opportunity.replacement, Replacement::Alias(_)).then_some("x");
            return Some(RegionRewrite {
                opportunity,
                rule,
                bindings,
                right_hand_variable,
                premises: Vec::new(),
            });
        }
    }
    fold_rewrite(event, matched, nodes, classes)
        .or_else(|| fact_rewrite(event, matched, classes, facts))
}

/// A canonical reduction of Section 8.11, re-derived against this region.
///
/// Section 11.6 lets a left-hand `const_like` match only a node already in the
/// before witness closure, so a reduction whose constant this transaction
/// derived is not this rule, whatever the sweep called it.
fn canonical_reduction(
    event: &AppliedRewrite,
    matched: &EvpNode,
    nodes: &[EvpNode],
    value_classes: &HashMap<u32, u32>,
    classes: &LocalClasses,
) -> Option<RewriteOpportunity> {
    let original = RewriteOpportunity {
        rule: event.rule,
        replacement: event.replacement.clone(),
    };
    let required_before_constants = match (&event.instruction, event.rule, &event.replacement) {
        (Inst::Add(_, left, right), ScalarRewrite::AddZero, Replacement::Alias(value))
        | (Inst::Or(_, left, right), ScalarRewrite::OrZero, Replacement::Alias(value)) => {
            if value == left {
                vec![(right, 0)]
            } else if value == right {
                vec![(left, 0)]
            } else {
                return None;
            }
        }
        (Inst::Mul(_, left, right), ScalarRewrite::MulOne, Replacement::Alias(value)) => {
            if value == left {
                vec![(right, 1)]
            } else if value == right {
                vec![(left, 1)]
            } else {
                return None;
            }
        }
        (Inst::And(_, left, right), ScalarRewrite::AndZero, Replacement::Constant(0)) => {
            let mut values = Vec::new();
            if is_constant(left, &event.facts, 0) {
                values.push((left, 0));
            }
            if is_constant(right, &event.facts, 0) {
                values.push((right, 0));
            }
            values
        }
        (_, ScalarRewrite::IdentityComparison, _) => {
            // `icmp.*(?x, ?x)` is a claim about one class, not about two
            // values the sweep happened to give one name. The sweep matches on
            // its own rewritten stream, where an earlier alias may already have
            // replaced one operand by the other; the region is built from
            // `before.evo`, and there the two operands are whatever classes
            // they are. A seed whose fact is fully known is still its own
            // class -- a fact is not a node, and no rule in this profile joins
            // a class to the constant its fact names -- so the identity has to
            // be read off the partition rather than off the instruction.
            let [left, right] = matched.children.as_slice() else {
                return None;
            };
            return (classes.find(*left)? == classes.find(*right)?).then_some(original);
        }
        (_, ScalarRewrite::DoubleNegation, _) => {
            return Some(original);
        }
        _ => return None,
    };
    required_before_constants
        .into_iter()
        .any(|(value, raw)| {
            class_has_before_constant(value, raw, nodes, value_classes, classes).unwrap_or(false)
        })
        .then_some(original)
}

/// `all_const` over the emitted node stream.
///
/// Section 11.6 puts no before-witness restriction on `all_const`, unlike the
/// `const_like` a canonical reduction matches: a class holds one unambiguous
/// constant of the required sort or it does not, whoever put it there.
fn fold_rewrite(
    event: &AppliedRewrite,
    matched: &EvpNode,
    nodes: &[EvpNode],
    classes: &LocalClasses,
) -> Option<RegionRewrite> {
    let operator = ingest_scalar_inst(&event.instruction).ok()?;
    let [left_class, right_class] = matched.children.as_slice() else {
        return None;
    };
    let operand_ty = scalar_operands(&event.instruction).first()?.ty.clone();
    let sort = packed_type(&operand_ty)?;
    let left = class_constant(nodes, classes, *left_class, &sort)?;
    let right = class_constant(nodes, classes, *right_class, &sort)?;
    let folded = fold_raw(operator, left, right, &operand_ty, &event.result.ty)?;
    Some(RegionRewrite {
        opportunity: RewriteOpportunity {
            rule: ScalarRewrite::ConstantFold,
            replacement: Replacement::Constant(folded),
        },
        rule: fold_rule(operator)?,
        bindings: operand_bindings(),
        right_hand_variable: None,
        premises: Vec::new(),
    })
}

/// The fact-driven rules of Section 11.6, decided by this region's facts.
fn fact_rewrite(
    event: &AppliedRewrite,
    matched: &EvpNode,
    classes: &LocalClasses,
    facts: &RegionFacts,
) -> Option<RegionRewrite> {
    let operator = ingest_scalar_inst(&event.instruction).ok()?;
    let fact = facts.of(classes.find(matched.owner)?, classes)?;
    if fact.is_constant() {
        // `zext(?a)`, `sext(?a)` and `trunc(?a)` name one variable; every other
        // fact-driven fold names two.
        let bindings = match operator {
            ScalarOperator::ZExt | ScalarOperator::SExt | ScalarOperator::Trunc => {
                vec![("a", BindingPath::child(LEFT))]
            }
            // The registry writes a shift's amount `?k`, not `?b`, and Section
            // 11.3 requires the binding names to be exactly the rule's own.
            ScalarOperator::Shl | ScalarOperator::LShr | ScalarOperator::AShr => vec![
                ("a", BindingPath::child(LEFT)),
                ("k", BindingPath::child(RIGHT)),
            ],
            _ => operand_bindings(),
        };
        return Some(RegionRewrite {
            opportunity: RewriteOpportunity {
                rule: ScalarRewrite::KnownConst,
                replacement: Replacement::Constant(fact.value()),
            },
            rule: known_const_rule(operator)?,
            bindings,
            right_hand_variable: None,
            premises: vec![PremiseSource::Matched],
        });
    }
    let [left_class, right_class] = matched.children.as_slice() else {
        return None;
    };
    let left_fact = facts.of(*left_class, classes)?;
    let right_fact = facts.of(*right_class, classes)?;
    let operands = scalar_operands(&event.instruction);
    let [left, right] = operands.as_slice() else {
        return None;
    };
    match operator {
        ScalarOperator::And if right_fact.covers(left_fact) => Some(cover_rewrite(
            "p1_220_and_cover_rhs",
            RIGHT,
            LEFT,
            "x",
            left,
        )),
        ScalarOperator::And if left_fact.covers(right_fact) => Some(cover_rewrite(
            "p1_221_and_cover_lhs",
            LEFT,
            RIGHT,
            "x",
            right,
        )),
        ScalarOperator::Or if right_fact.covers(left_fact) => Some(cover_rewrite(
            "p1_222_or_cover_rhs",
            RIGHT,
            LEFT,
            "m",
            right,
        )),
        ScalarOperator::Or if left_fact.covers(right_fact) => {
            Some(cover_rewrite("p1_223_or_cover_lhs", LEFT, RIGHT, "m", left))
        }
        ScalarOperator::Xor if right_fact.is_known_zero() => {
            Some(zero_rewrite("p1_224_xor_known_zero_rhs", LEFT, RIGHT, left))
        }
        ScalarOperator::Xor if left_fact.is_known_zero() => Some(zero_rewrite(
            "p1_225_xor_known_zero_lhs",
            RIGHT,
            LEFT,
            right,
        )),
        _ => None,
    }
}

/// One `covers` rule: `?m` at child `mask`, `?x` at child `value`, and the
/// operand the right-hand side names.
fn cover_rewrite(
    rule: &'static str,
    mask: usize,
    value: usize,
    right_hand_variable: &'static str,
    result: &Value,
) -> RegionRewrite {
    RegionRewrite {
        opportunity: RewriteOpportunity {
            rule: ScalarRewrite::MaskCover,
            replacement: Replacement::Alias(result.clone()),
        },
        rule,
        bindings: vec![
            ("m", BindingPath::child(mask)),
            ("x", BindingPath::child(value)),
        ],
        right_hand_variable: Some(right_hand_variable),
        premises: vec![PremiseSource::Variable("m"), PremiseSource::Variable("x")],
    }
}

/// One `known_zero` rule: `?x` at child `kept`, `?z` at child `zero`.
fn zero_rewrite(rule: &'static str, kept: usize, zero: usize, result: &Value) -> RegionRewrite {
    RegionRewrite {
        opportunity: RewriteOpportunity {
            rule: ScalarRewrite::XorKnownZero,
            replacement: Replacement::Alias(result.clone()),
        },
        rule,
        bindings: vec![
            ("x", BindingPath::child(kept)),
            ("z", BindingPath::child(zero)),
        ],
        right_hand_variable: Some("x"),
        premises: vec![PremiseSource::Variable("z")],
    }
}

fn operand_bindings() -> Vec<(&'static str, BindingPath)> {
    vec![
        ("a", BindingPath::child(LEFT)),
        ("b", BindingPath::child(RIGHT)),
    ]
}

fn fold_rule(operator: ScalarOperator) -> Option<&'static str> {
    Some(match operator {
        ScalarOperator::Add => "p0_100_fold_add",
        ScalarOperator::Sub => "p0_101_fold_sub",
        ScalarOperator::Mul => "p0_102_fold_mul",
        ScalarOperator::And => "p0_103_fold_and",
        ScalarOperator::Or => "p0_104_fold_or",
        ScalarOperator::Xor => "p0_105_fold_xor",
        ScalarOperator::ICmp(CmpOp::Eq) => "p0_106_fold_icmp_eq",
        ScalarOperator::ICmp(CmpOp::Ne) => "p0_107_fold_icmp_ne",
        ScalarOperator::ICmp(CmpOp::SLt) => "p0_108_fold_icmp_slt",
        ScalarOperator::ICmp(CmpOp::SLe) => "p0_109_fold_icmp_sle",
        ScalarOperator::ICmp(CmpOp::ULt) => "p0_110_fold_icmp_ult",
        ScalarOperator::ICmp(CmpOp::ULe) => "p0_111_fold_icmp_ule",
        // The registry gives these no `all_const` fold; their exact value is a
        // fact, and `known_const_rule` is what states it.
        ScalarOperator::Const
        | ScalarOperator::Shl
        | ScalarOperator::LShr
        | ScalarOperator::AShr
        | ScalarOperator::ZExt
        | ScalarOperator::SExt
        | ScalarOperator::Trunc => return None,
    })
}

fn known_const_rule(operator: ScalarOperator) -> Option<&'static str> {
    Some(match operator {
        ScalarOperator::And => "p1_200_known_and",
        ScalarOperator::Or => "p1_201_known_or",
        ScalarOperator::Xor => "p1_202_known_xor",
        ScalarOperator::Add => "p1_203_known_add",
        ScalarOperator::Sub => "p1_204_known_sub",
        ScalarOperator::Mul => "p1_205_known_mul",
        ScalarOperator::ICmp(CmpOp::Eq) => "p1_210_known_icmp_eq",
        ScalarOperator::ICmp(CmpOp::Ne) => "p1_211_known_icmp_ne",
        ScalarOperator::ICmp(CmpOp::SLt) => "p1_212_known_icmp_slt",
        ScalarOperator::ICmp(CmpOp::SLe) => "p1_213_known_icmp_sle",
        ScalarOperator::ICmp(CmpOp::ULt) => "p1_214_known_icmp_ult",
        ScalarOperator::ICmp(CmpOp::ULe) => "p1_215_known_icmp_ule",
        ScalarOperator::Shl => "p1_206_known_shl",
        ScalarOperator::LShr => "p1_207_known_lshr",
        ScalarOperator::AShr => "p1_208_known_ashr",
        ScalarOperator::ZExt => "p1_230_known_zext",
        ScalarOperator::SExt => "p1_231_known_sext",
        ScalarOperator::Trunc => "p1_232_known_trunc",
        ScalarOperator::Const => return None,
    })
}

/// The one constant a class holds of `sort`, if it holds exactly one.
///
/// A class holding two different constants is not a choice: Section 11.6 makes
/// it invalid, so this answers `None` rather than picking.
fn class_constant(
    nodes: &[EvpNode],
    classes: &LocalClasses,
    class: u32,
    sort: &str,
) -> Option<u64> {
    let root = classes.find(class)?;
    let mut found: Option<u64> = None;
    for node in nodes {
        if node.operation != "const" || node.sort != sort || classes.find(node.owner) != Some(root)
        {
            continue;
        }
        let [(name, raw)] = node.attributes.as_slice() else {
            return None;
        };
        if name != "value" || found.is_some_and(|earlier| earlier != *raw) {
            return None;
        }
        found = Some(*raw);
    }
    found
}

/// The Section 8.10 fact of every class of one region, recomputed from its
/// nodes.
///
/// A class's fact is the meet of the transfers of the nodes it owns and, for a
/// seed class, of the fact Section 8.10's function-wide fixed point gives the
/// block argument. A meet that disagrees with itself on a known bit is an
/// invalid union rather than a fact, so the whole region is abandoned. The producer computes this the way
/// the checker will, from the node stream and nothing else: a premise derived
/// from the sweep's own environment would be a claim only the producer could
/// make.
struct RegionFacts {
    facts: Vec<KnownBits>,
}

impl RegionFacts {
    fn compute(
        nodes: &[EvpNode],
        widths: &[u16],
        seeds: &[KnownBits],
        classes: &LocalClasses,
    ) -> Option<Self> {
        let mut facts = widths
            .iter()
            .map(|width| KnownBits::top(*width))
            .collect::<Vec<_>>();
        // Seed classes come first, in manifest order, and start at the fact
        // Section 8.10 gives the block argument they name.
        for (index, fact) in seeds.iter().enumerate() {
            let root = classes.find(u32::try_from(index).ok()?)? as usize;
            facts[root] = facts.get(root).copied()?.meet(*fact)?;
        }
        loop {
            let mut changed = false;
            for node in nodes {
                let transferred = node_transfer(node, &facts, classes)?;
                let root = classes.find(node.owner)? as usize;
                let merged = facts.get(root).copied()?.meet(transferred)?;
                if merged != facts[root] {
                    facts[root] = merged;
                    changed = true;
                }
            }
            if !changed {
                return Some(Self { facts });
            }
        }
    }

    fn of(&self, class: u32, classes: &LocalClasses) -> Option<KnownBits> {
        self.facts.get(classes.find(class)? as usize).copied()
    }
}

/// One row of the Section 8.10 transfer table, applied to one node.
fn node_transfer(node: &EvpNode, facts: &[KnownBits], classes: &LocalClasses) -> Option<KnownBits> {
    let width = sort_width(&node.sort)?;
    let child = |index: usize| -> Option<KnownBits> {
        let class = classes.find(*node.children.get(index)?)?;
        facts.get(class as usize).copied()
    };
    let fact = match node.operation.as_str() {
        "const" => {
            let [(name, raw)] = node.attributes.as_slice() else {
                return None;
            };
            if name != "value" {
                return None;
            }
            KnownBits::constant(width, *raw)
        }
        "and" => child(LEFT)?.transfer_and(child(RIGHT)?),
        "or" => child(LEFT)?.transfer_or(child(RIGHT)?),
        "xor" => child(LEFT)?.transfer_xor(child(RIGHT)?),
        "add" => child(LEFT)?.transfer_add(child(RIGHT)?),
        "sub" => child(LEFT)?.transfer_sub(child(RIGHT)?),
        "mul" => child(LEFT)?.transfer_mul(child(RIGHT)?),
        "icmp.eq" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::Eq),
        "icmp.ne" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::Ne),
        "icmp.slt" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::SLt),
        "icmp.sle" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::SLe),
        "icmp.ult" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::ULt),
        "icmp.ule" => child(LEFT)?.transfer_icmp(child(RIGHT)?, Comparison::ULe),
        "shl" => child(LEFT)?.transfer_shl(child(RIGHT)?),
        "lshr" => child(LEFT)?.transfer_lshr(child(RIGHT)?),
        "ashr" => child(LEFT)?.transfer_ashr(child(RIGHT)?),
        "zext" => child(LEFT)?.transfer_zext(width),
        "sext" => child(LEFT)?.transfer_sext(width),
        "trunc" => child(LEFT)?.transfer_trunc(width),
        _ => return None,
    };
    Some(fact)
}

fn class_has_before_constant(
    value: &Value,
    raw: u64,
    nodes: &[EvpNode],
    value_classes: &HashMap<u32, u32>,
    classes: &LocalClasses,
) -> Option<bool> {
    let class = classes.find(*value_classes.get(&value.id)?)?;
    let raw = mask_raw(raw, &value.ty);
    Some(nodes.iter().any(|node| {
        node.source == EvpNodeSource::Before
            && node.operation == "const"
            && classes.find(node.owner) == Some(class)
            && matches!(node.attributes.as_slice(), [(name, value)] if name == "value" && *value == raw)
    }))
}

fn collect_closure(
    block: &evocc_evoir::Block,
    definitions: &HashMap<u32, usize>,
    index: usize,
    closure: &mut HashSet<usize>,
) -> Option<()> {
    if !closure.insert(index) {
        return Some(());
    }
    ingest_scalar_inst(&block.insts[index]).ok()?;
    for operand in scalar_operands(&block.insts[index]) {
        if let Some(definition) = definitions.get(&operand.id) {
            if *definition >= index {
                return None;
            }
            ingest_scalar_inst(&block.insts[*definition]).ok()?;
            collect_closure(block, definitions, *definition, closure)?;
        }
    }
    Some(())
}

/// What makes two instructions the same node: the operation, the sort it
/// produces, its attributes, and its operand values in order.
pub(crate) type InstructionShape = (ScalarOperator, EvoIRType, u64, Vec<u32>);

/// One block's shapes, and where each is written.
///
/// Computed once for the block rather than once per candidate root: with the
/// duplicate rule of revision 1.9.44 every region of a block asks the same
/// question, and asking it per region is what made a thousand programs stop
/// finishing.
pub(crate) struct BlockShapes {
    shapes: Vec<Option<InstructionShape>>,
    written_at: HashMap<InstructionShape, Vec<usize>>,
}

pub(crate) fn block_shapes(block: &evocc_evoir::Block) -> BlockShapes {
    let shapes = block
        .insts
        .iter()
        .map(instruction_shape)
        .collect::<Vec<_>>();
    let mut written_at: HashMap<InstructionShape, Vec<usize>> = HashMap::new();
    for (index, shape) in shapes.iter().enumerate() {
        if let Some(shape) = shape {
            written_at.entry(shape.clone()).or_default().push(index);
        }
    }
    BlockShapes { shapes, written_at }
}

/// Section 8.5 as revised at 1.9.44: every allowlisted instruction of the same
/// block, earlier than the root, that is a structural duplicate of one already
/// in the closure joins it, each with its own operand cone, to a fixed point.
///
/// A duplicate's cone is its twin's cone, since the operands are the same
/// values, so nothing outside the closure's own seeds can arrive this way.
fn join_duplicates(
    block: &evocc_evoir::Block,
    definitions: &HashMap<u32, usize>,
    shapes: &BlockShapes,
    root: usize,
    closure: &mut HashSet<usize>,
) -> Option<()> {
    loop {
        let mut joining = Vec::new();
        for index in closure.iter() {
            let Some(shape) = shapes.shapes.get(*index).and_then(Option::as_ref) else {
                continue;
            };
            for candidate in shapes.written_at.get(shape).into_iter().flatten() {
                if *candidate < root && !closure.contains(candidate) {
                    joining.push(*candidate);
                }
            }
        }
        if joining.is_empty() {
            return Some(());
        }
        joining.sort_unstable();
        joining.dedup();
        for index in joining {
            collect_closure(block, definitions, index, closure)?;
        }
    }
}

fn instruction_shape(instruction: &Inst) -> Option<InstructionShape> {
    let operator = ingest_scalar_inst(instruction).ok()?;
    let payload = match instruction {
        Inst::Const(value, raw) => mask_raw(*raw, &value.ty),
        _ => 0,
    };
    Some((
        operator,
        crate::scalar_eq::scalar_result(instruction)?.ty.clone(),
        payload,
        scalar_operands(instruction)
            .into_iter()
            .map(|operand| operand.id)
            .collect(),
    ))
}

fn before_node(
    instruction: &Inst,
    node: u32,
    owner: u32,
    values: &HashMap<u32, u32>,
) -> Option<EvpNode> {
    let result = inst_results(instruction).first().copied()?;
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
        _ => return None,
    };
    Some(EvpNode {
        node,
        owner,
        source: EvpNodeSource::Before,
        operation: operation.to_owned(),
        sort: packed_type(&result.ty)?,
        effect: EvpEffectClass::Pure,
        attributes,
        children: operands
            .into_iter()
            .map(|operand| values.get(&operand.id).copied())
            .collect::<Option<_>>()?,
    })
}

/// Where the node stream puts one rule variable.
///
/// Section 11.3 requires a binding to equal the class a checker derives from
/// the operation and children of the node the rewrite names, after the merges
/// recorded before it. A path into that node is therefore the only admissible
/// description of a variable; an operand of the instruction is not, because a
/// later replacement in the same witness closure may have remapped it onto a
/// different class holding an equal constant.
#[derive(Clone, Copy)]
struct BindingPath {
    child: usize,
    through: Option<Nested>,
}

/// A left pattern that passes through a second node on the way to a variable.
///
/// The whole pattern has to be matched, not just its operator: a class may own
/// several nodes of the same operation once merges have run, and only the one
/// whose remaining child holds the constant the pattern writes there is the
/// node the rule matched. Resolving the variable from any other is a binding
/// the checker recomputes differently and refuses.
#[derive(Clone, Copy)]
struct Nested {
    operation: &'static str,
    /// The child of that node the variable names.
    inner: usize,
    /// The child the pattern fixes, and the constant it fixes it to.
    fixed: usize,
    constant: u64,
}

const LEFT: usize = 0;
const RIGHT: usize = 1;

impl BindingPath {
    const fn child(child: usize) -> Self {
        Self {
            child,
            through: None,
        }
    }

    const fn nested(child: usize, through: Nested) -> Self {
        Self {
            child,
            through: Some(through),
        }
    }
}

/// Resolve one variable against the emitted node stream and the partition as it
/// stands before this rewrite.
fn binding_class(
    nodes: &[EvpNode],
    classes: &LocalClasses,
    node: u32,
    path: BindingPath,
) -> Option<u32> {
    let named = nodes.get(node as usize)?;
    let class = classes.find(*named.children.get(path.child)?)?;
    let Some(nested) = path.through else {
        return Some(class);
    };
    nodes
        .iter()
        .filter(|candidate| {
            candidate.source == EvpNodeSource::Before
                && candidate.operation == nested.operation
                && candidate.sort == named.sort
                && classes.find(candidate.owner) == Some(class)
        })
        .find_map(|candidate| {
            let fixed = *candidate.children.get(nested.fixed)?;
            if !class_holds_before_constant(nodes, classes, fixed, &candidate.sort, nested.constant)
            {
                return None;
            }
            classes.find(*candidate.children.get(nested.inner)?)
        })
}

/// Whether a class owns a before-witness `const` of `sort` and `raw`.
fn class_holds_before_constant(
    nodes: &[EvpNode],
    classes: &LocalClasses,
    class: u32,
    sort: &str,
    raw: u64,
) -> bool {
    let Some(root) = classes.find(class) else {
        return false;
    };
    nodes.iter().any(|node| {
        node.source == EvpNodeSource::Before
            && node.operation == "const"
            && node.sort == sort
            && classes.find(node.owner) == Some(root)
            && matches!(node.attributes.as_slice(), [(name, value)] if name == "value" && *value == raw)
    })
}

fn exact_rule(
    instruction: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    negations: &NegationEnv,
    opportunity: &RewriteOpportunity,
) -> Option<(&'static str, Vec<(&'static str, BindingPath)>)> {
    let x = |child: usize| vec![("x", BindingPath::child(child))];
    let ab = || {
        vec![
            ("a", BindingPath::child(LEFT)),
            ("b", BindingPath::child(RIGHT)),
        ]
    };
    match (instruction, opportunity.rule, &opportunity.replacement) {
        (Inst::Add(_, left, right), ScalarRewrite::AddZero, Replacement::Alias(value)) => {
            if value == left {
                Some(("p0_001_add_zero_rhs", x(LEFT)))
            } else if value == right {
                Some(("p0_002_add_zero_lhs", x(RIGHT)))
            } else {
                None
            }
        }
        (Inst::Mul(_, left, right), ScalarRewrite::MulOne, Replacement::Alias(value)) => {
            if value == left {
                Some(("p0_003_mul_one_rhs", x(LEFT)))
            } else if value == right {
                Some(("p0_004_mul_one_lhs", x(RIGHT)))
            } else {
                None
            }
        }
        (Inst::And(_, left, right), ScalarRewrite::AndZero, Replacement::Constant(0)) => {
            if is_constant(right, facts, 0) {
                Some(("p0_005_and_zero_rhs", x(LEFT)))
            } else if is_constant(left, facts, 0) {
                Some(("p0_006_and_zero_lhs", x(RIGHT)))
            } else {
                None
            }
        }
        (Inst::Or(_, left, right), ScalarRewrite::OrZero, Replacement::Alias(value)) => {
            if value == left {
                Some(("p0_007_or_zero_rhs", x(LEFT)))
            } else if value == right {
                Some(("p0_008_or_zero_lhs", x(RIGHT)))
            } else {
                None
            }
        }
        (Inst::Sub(_, _, right), ScalarRewrite::DoubleNegation, Replacement::Alias(value)) => {
            negations
                .get(&right.id)
                .is_some_and(|operand| operand == value)
                .then(|| {
                    (
                        "p0_009_double_neg",
                        vec![(
                            "x",
                            BindingPath::nested(
                                RIGHT,
                                Nested {
                                    operation: "sub",
                                    inner: RIGHT,
                                    fixed: LEFT,
                                    constant: 0,
                                },
                            ),
                        )],
                    )
                })
        }
        (
            Inst::ICmp(_, comparison, left, right),
            ScalarRewrite::IdentityComparison,
            Replacement::Constant(_),
        ) if left == right => Some((
            match comparison {
                CmpOp::Eq => "p0_010_icmp_eq_self",
                CmpOp::Ne => "p0_011_icmp_ne_self",
                CmpOp::SLt => "p0_012_icmp_slt_self",
                CmpOp::SLe => "p0_013_icmp_sle_self",
                CmpOp::ULt => "p0_014_icmp_ult_self",
                CmpOp::ULe => "p0_015_icmp_ule_self",
            },
            x(LEFT),
        )),
        (instruction, ScalarRewrite::ConstantFold, Replacement::Constant(_)) => {
            let operands = scalar_operands(instruction);
            let [_left, _right] = operands.as_slice() else {
                return None;
            };
            let rule = match instruction {
                Inst::Add(..) => "p0_100_fold_add",
                Inst::Sub(..) => "p0_101_fold_sub",
                Inst::Mul(..) => "p0_102_fold_mul",
                Inst::And(..) => "p0_103_fold_and",
                Inst::Or(..) => "p0_104_fold_or",
                Inst::Xor(..) => "p0_105_fold_xor",
                Inst::ICmp(_, CmpOp::Eq, ..) => "p0_106_fold_icmp_eq",
                Inst::ICmp(_, CmpOp::Ne, ..) => "p0_107_fold_icmp_ne",
                Inst::ICmp(_, CmpOp::SLt, ..) => "p0_108_fold_icmp_slt",
                Inst::ICmp(_, CmpOp::SLe, ..) => "p0_109_fold_icmp_sle",
                Inst::ICmp(_, CmpOp::ULt, ..) => "p0_110_fold_icmp_ult",
                Inst::ICmp(_, CmpOp::ULe, ..) => "p0_111_fold_icmp_ule",
                _ => return None,
            };
            Some((rule, ab()))
        }
        _ => None,
    }
}

fn is_constant(value: &Value, facts: &HashMap<u32, ScalarFact>, raw: u64) -> bool {
    matches!(facts.get(&value.id), Some(ScalarFact::Constant { raw: actual, ty }) if ty == &value.ty && *actual == mask_raw(raw, ty))
}

enum SelectedSource {
    Value(u32),
    Node(u32),
}

/// A deterministic representative of the class an alias rewrite replaces with.
///
/// Seeds in manifest order, then nodes in emission order. The order is fixed
/// rather than convenient because a producer and a checker that never share a
/// data structure must still agree on the extraction this choice implies.
fn alias_representative(
    seed_values: &[Value],
    nodes: &[EvpNode],
    node_values: &HashMap<u32, Value>,
    classes: &LocalClasses,
    class: u32,
) -> Option<(SelectedSource, Replacement)> {
    for (index, value) in seed_values.iter().enumerate() {
        if classes.find(u32::try_from(index).ok()?) == Some(class) {
            return Some((
                SelectedSource::Value(value.id),
                Replacement::Alias(value.clone()),
            ));
        }
    }
    for node in nodes {
        if classes.find(node.owner) == Some(class) {
            let value = node_values.get(&node.node)?;
            return Some((
                SelectedSource::Node(node.node),
                Replacement::Alias(value.clone()),
            ));
        }
    }
    None
}

/// A member of `class` that costs less to extract than the node at `node_id`.
///
/// Seeds first, then nodes in emission order, skipping the node the region is
/// replacing: that one costs one, and reusing it would claim an improvement the
/// checker recomputes as none.
fn cheaper_member(
    seed_values: &[Value],
    nodes: &[EvpNode],
    node_values: &HashMap<u32, Value>,
    classes: &LocalClasses,
    class: u32,
    replaced: u32,
) -> Option<(SelectedSource, Replacement)> {
    for (index, value) in seed_values.iter().enumerate() {
        if classes.find(u32::try_from(index).ok()?) == Some(class) {
            return Some((
                SelectedSource::Value(value.id),
                Replacement::Alias(value.clone()),
            ));
        }
    }
    for node in nodes {
        if node.node == replaced || classes.find(node.owner) != Some(class) {
            continue;
        }
        let value = node_values.get(&node.node)?;
        return Some((
            SelectedSource::Node(node.node),
            Replacement::Alias(value.clone()),
        ));
    }
    None
}

fn extraction_manifest(
    nodes: &[EvpNode],
    seeds: &[EvpSeed],
    values: &HashMap<u32, u32>,
    selected: SelectedSource,
    replacement_root: u32,
    classes: &LocalClasses,
) -> Option<Vec<EvpExtraction>> {
    let (class, choice) = match selected {
        SelectedSource::Value(value) => {
            let class = *values.get(&value)?;
            if let Some(index) = seeds.iter().position(|seed| seed.value == value) {
                (class, EvpExtractionChoice::Seed(index as u32))
            } else {
                let node = nodes.iter().find(|node| node.owner == class)?.node;
                (class, EvpExtractionChoice::Node(node))
            }
        }
        SelectedSource::Node(node) => {
            let node = nodes.get(node as usize)?;
            (node.owner, EvpExtractionChoice::Node(node.node))
        }
    };
    let mut builder = ExtractionManifestBuilder {
        nodes,
        seeds,
        replacement_root,
        classes,
        seen: HashSet::new(),
        output: Vec::new(),
    };
    builder.append(class, choice)?;
    Some(builder.output)
}

struct ExtractionManifestBuilder<'a> {
    nodes: &'a [EvpNode],
    seeds: &'a [EvpSeed],
    replacement_root: u32,
    classes: &'a LocalClasses,
    seen: HashSet<u32>,
    output: Vec<EvpExtraction>,
}

impl ExtractionManifestBuilder<'_> {
    fn append(&mut self, class: u32, choice: EvpExtractionChoice) -> Option<()> {
        let final_class = self.classes.find(class)?;
        if self.seen.contains(&final_class) {
            return Some(());
        }
        if let EvpExtractionChoice::Node(node_id) = choice {
            let children = {
                let node = self.nodes.get(node_id as usize)?;
                let terminal_before_reuse =
                    node.source == EvpNodeSource::Before && node.owner != self.replacement_root;
                (!terminal_before_reuse).then(|| node.children.clone())
            };
            for child in children.into_iter().flatten() {
                let child_node = self.nodes.iter().find(|node| node.owner == child);
                let child_choice = if let Some(node) = child_node {
                    EvpExtractionChoice::Node(node.node)
                } else {
                    let seed = self.seeds.iter().position(|seed| seed.class == child)?;
                    EvpExtractionChoice::Seed(seed as u32)
                };
                self.append(child, child_choice)?;
            }
        }
        if self.seen.insert(final_class) {
            self.output.push(EvpExtraction {
                class: final_class,
                choice,
            });
        }
        Some(())
    }
}

fn apply(module: &Module, accepted: &[Accepted]) -> Result<(Module, ValueRemap), ()> {
    let mut output = module.clone();
    let mut value_remap = HashMap::new();
    for (function_index, function) in output.functions.iter_mut().enumerate() {
        let mut aliases = HashMap::new();
        for (block_index, block) in function.blocks.iter_mut().enumerate() {
            let mut rewritten = Vec::new();
            for (instruction_index, mut instruction) in
                std::mem::take(&mut block.insts).into_iter().enumerate()
            {
                let accepted = accepted.iter().find(|accepted| {
                    accepted.function == function_index
                        && accepted.block == block_index
                        && accepted.instruction == instruction_index
                });
                if let Some(accepted) = accepted {
                    if inst_results(&instruction)
                        .first()
                        .is_none_or(|result| result.id != accepted.root)
                    {
                        return Err(());
                    }
                    match &accepted.replacement {
                        Replacement::Alias(value) => {
                            aliases.insert(accepted.root, resolve_alias(value, &aliases));
                        }
                        Replacement::Constant(raw) => {
                            let result = inst_results(&instruction)[0].clone();
                            rewritten.push(Inst::Const(result.clone(), mask_raw(*raw, &result.ty)));
                        }
                    }
                } else {
                    remap_inst_uses(&mut instruction, &aliases);
                    rewritten.push(instruction);
                }
            }
            block.insts = rewritten;
            remap_inst_uses(&mut block.terminator, &aliases);
        }
        let identities = renumber(function)?;
        for (old, new) in identities {
            value_remap.insert((function.name.clone(), old), new);
        }
    }
    validate_module(&output).map_err(|_| ())?;
    Ok((output, value_remap))
}

fn renumber(function: &mut evocc_evoir::Function) -> Result<HashMap<u32, u32>, ()> {
    let mut identities = HashMap::new();
    let mut next = 0_u32;
    for block in &function.blocks {
        for argument in &block.args {
            if identities.insert(argument.value.id, next).is_some() {
                return Err(());
            }
            next = next.checked_add(1).ok_or(())?;
        }
        for instruction in &block.insts {
            for result in inst_results(instruction) {
                if identities.insert(result.id, next).is_some() {
                    return Err(());
                }
                next = next.checked_add(1).ok_or(())?;
            }
        }
    }
    for block in &mut function.blocks {
        for argument in &mut block.args {
            remap_value(&mut argument.value, &identities)?;
        }
        for instruction in &mut block.insts {
            remap_all(instruction, &identities)?;
        }
        remap_all(&mut block.terminator, &identities)?;
    }
    Ok(identities)
}

fn remap_value(value: &mut Value, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    value.id = *identities.get(&value.id).ok_or(())?;
    Ok(())
}

fn remap_all(instruction: &mut Inst, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    for value in all_values_mut(instruction) {
        remap_value(value, identities)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn all_values_mut(instruction: &mut Inst) -> Vec<&mut Value> {
    match instruction {
        Inst::Const(result, _) | Inst::Uninitialized(result) => vec![result],
        Inst::Add(result, left, right)
        | Inst::Sub(result, left, right)
        | Inst::Mul(result, left, right)
        | Inst::SRem(result, left, right)
        | Inst::URem(result, left, right)
        | Inst::Shl(result, left, right)
        | Inst::AShr(result, left, right)
        | Inst::LShr(result, left, right)
        | Inst::And(result, left, right)
        | Inst::Or(result, left, right)
        | Inst::Xor(result, left, right)
        | Inst::SignedAddDefined(result, left, right)
        | Inst::SignedSubDefined(result, left, right)
        | Inst::SignedMulDefined(result, left, right)
        | Inst::SignedDivRemNoOverflow(result, left, right)
        | Inst::SDiv(result, left, right)
        | Inst::UDiv(result, left, right)
        | Inst::ICmp(result, _, left, right) => vec![result, left, right],
        Inst::DivisorNonZero(result, divisor) => vec![result, divisor],
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        }
        | Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            ..
        } => vec![result, shifted, amount],
        Inst::UbAssume(condition, _) => vec![condition],
        Inst::Trap(_) => Vec::new(),
        Inst::SExt(result, source, _)
        | Inst::ZExt(result, source, _)
        | Inst::Trunc(result, source, _) => vec![result, source],
        Inst::PtrNull(result) | Inst::GlobalAddr { result, .. } | Inst::FuncAddr { result, .. } => {
            vec![result]
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            ..
        } => vec![result, base, index],
        Inst::Br(_, values) | Inst::Ret(values) => values.iter_mut().collect(),
        Inst::CondBr(condition, _, left, _, right) => {
            let mut values = vec![condition];
            values.extend(left);
            values.extend(right);
            values
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_in, world_in, mem_out, world_out],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => vec![mem_in, world_in, pointer, mem_out, world_out],
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        }
        | Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            ..
        } => vec![mem_in, pointer, mem_out],
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, address, mem_in, world_in, mem_out, world_out],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![address, value, mem_in, world_in, mem_out, world_out],
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![destination, source, mem_in, world_in, mem_out, world_out],
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.extend(args);
            values.extend([mem_in, world_in, mem_out, world_out]);
            values
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.push(callee);
            values.extend(args);
            values.extend([mem_in, world_in, mem_out, world_out]);
            values
        }
    }
}

/// The width a packed `bits<N>` sort names.
fn sort_width(sort: &str) -> Option<u16> {
    let width = sort
        .strip_prefix("bits<")?
        .strip_suffix('>')?
        .parse()
        .ok()?;
    (1..=64).contains(&width).then_some(width)
}

fn packed_type(ty: &EvoIRType) -> Option<String> {
    match ty {
        EvoIRType::Bits(width @ 1..=64) => Some(format!("bits<{width}>")),
        _ => None,
    }
}
