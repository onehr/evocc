//! Opt-in g0 certificate production, using only the existing P0 operand cones.
//! Unlike the diagnostic lane, excluded/opaque operands never enter a witness.
use super::*;

pub(super) fn optimize(
    module: &Module,
    policy: &ExtractionPolicy,
    target: &str,
) -> CertifiedScalarEqTransaction {
    let input = render_target_module(module, target);
    let mut report = ScalarEqReport::default();
    let mut regions = Vec::new();
    let mut events = Vec::new();
    let mut accepted = Vec::new();
    let mut proof_nodes = 0usize;
    let budget = policy.schedule().budget();
    let admissions: Vec<_> = module.functions.iter().map(Admission::of).collect();
    // Inventory the whole input even if certificate admission later stops.
    for (function, admission) in module.functions.iter().zip(&admissions) {
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            let Some((_, _, operands)) = guard_instruction(inst) else {
                continue;
            };
            report.telemetry.roots_total += 1;
            if admission.is_tainted(inst_results(inst)[0].id) {
                report.telemetry.roots_tainted += 1;
            } else if operands.iter().all(|v| admission.admits(v.id)) {
                report.telemetry.roots_eligible += 1;
            } else {
                report.telemetry.roots_excluded_dependency += 1;
            }
        }
    }
    'scan: for (fi, function) in module.functions.iter().enumerate() {
        let admission = &admissions[fi];
        let seed_facts = SeedFacts::of(function, admission);
        report.seed_fact_work_items += seed_facts.work_items();
        for (bi, block) in function.blocks.iter().enumerate() {
            let context = Surroundings {
                shapes: block_shapes(block),
                admission,
                seed_facts: &seed_facts,
            };
            for (ii, inst) in block.insts.iter().enumerate() {
                if guard_instruction(inst).is_none() {
                    continue;
                }
                report.telemetry.roots_visited += 1;
                let result = inst_results(inst)[0];
                if admission.is_tainted(result.id) {
                    continue;
                }
                let Some((region, event)) = build(module, fi, bi, ii, &context) else {
                    continue;
                };
                report.telemetry.roots_proposed += 1;
                let count = region
                    .events
                    .iter()
                    .filter(|e| matches!(e, EvpEvent::Node(_)))
                    .count();
                if regions.len() >= budget.max_epochs
                    || proof_nodes.saturating_add(count) > budget.max_nodes
                {
                    report.budget_exhausted = true;
                    report.telemetry.admission_regions_hit = regions.len() >= budget.max_epochs;
                    report.telemetry.admission_nodes_hit =
                        proof_nodes.saturating_add(count) > budget.max_nodes;
                    report.telemetry.admission_next_nodes = count;
                    break 'scan;
                }
                report.policy_decisions.push(crate::PolicyDecision {
                    function_index: fi,
                    block: block.id,
                    root: result.id,
                    root_axis: None,
                    replacement_axis: None,
                    root_cost: 1,
                    replacement_cost: 0,
                    registry_admitted: true,
                    kept: true,
                });
                accepted.push(Accepted {
                    function: fi,
                    block: bi,
                    instruction: ii,
                    root: result.id,
                    replacement: Replacement::Constant(1),
                });
                proof_nodes += count;
                events.push(event);
                regions.push(region);
            }
        }
    }
    let (output, value_remap) = if accepted.is_empty() {
        (module.clone(), HashMap::new())
    } else if let Ok(output) = apply(module, &accepted) {
        output
    } else {
        report.fell_back = true;
        report.internal_error = Some("guard root splice failed validation".to_owned());
        let mut zero = zero_transaction(module, input, report);
        zero.outcome.certificate.registry_name = GUARD_REGISTRY.to_owned();
        zero.outcome.certificate.registry_digest = GUARD_DIGEST.to_owned();
        zero.outcome.report.proof_authority = ProofAuthority::Reviewed {
            registry: GUARD_REGISTRY,
        };
        return zero;
    };
    report.proof_authority = ProofAuthority::Reviewed {
        registry: GUARD_REGISTRY,
    };
    report.regions_admitted = regions.len();
    report.graph_nodes = proof_nodes;
    report.epochs = regions.len();
    report.merges = regions.len();
    report.optimized_values = regions.len();
    report.telemetry.admission_regions = regions.len();
    report.telemetry.admission_proof_nodes = proof_nodes;
    report.congruences = regions
        .iter()
        .flat_map(|r| &r.events)
        .filter(|e| matches!(e, EvpEvent::Congruence(_)))
        .count();
    let certificate = EvpCertificate {
        version: EvpVersion::Evp2,
        registry_name: GUARD_REGISTRY.to_owned(),
        registry_digest: GUARD_DIGEST.to_owned(),
        input_digest: sha256_hex(input.as_bytes()),
        output_digest: sha256_hex(render_target_module(&output, target).as_bytes()),
        budget: Some(scheduler_record(&regions, report.budget_exhausted)),
        regions,
    };
    // Use the existing mechanical reassembly and canonical renumbering.
    CertifiedScalarEqTransaction {
        outcome: CertifiedScalarEqOutcome {
            module: output,
            certificate,
            report,
        },
        events,
        value_remap,
    }
}

fn build(
    module: &Module,
    fi: usize,
    bi: usize,
    ii: usize,
    surroundings: &Surroundings,
) -> Option<(EvpRegion, AppliedRewrite)> {
    let function = &module.functions[fi];
    let block = &function.blocks[bi];
    let instruction = &block.insts[ii];
    let (_, rule, operands) = guard_instruction(instruction)?;
    let result = inst_results(instruction)[0];
    if surroundings.admission.is_tainted(result.id)
        || !operands.iter().all(|v| surroundings.admission.admits(v.id))
    {
        return None;
    }
    let definitions: HashMap<_, _> = block
        .insts
        .iter()
        .enumerate()
        .flat_map(|(i, inst)| inst_results(inst).into_iter().map(move |v| (v.id, i)))
        .collect();
    let mut closure = HashSet::from([ii]);
    for operand in &operands {
        if let Some(&definition) = definitions.get(&operand.id) {
            if definition >= ii {
                return None;
            }
            collect_closure(block, &definitions, definition, &mut closure)?;
        }
    }
    join_duplicates(block, &definitions, &surroundings.shapes, ii, &mut closure)?;
    let witnesses: Vec<_> = (0..block.insts.len())
        .filter(|i| closure.contains(i))
        .collect();
    let uses = |i: usize| {
        if i == ii {
            operands.clone()
        } else {
            scalar_operands(&block.insts[i])
        }
    };
    let mut seeds = Vec::new();
    let mut seen = HashSet::new();
    for &i in &witnesses {
        for value in uses(i) {
            if !definitions
                .get(&value.id)
                .is_some_and(|d| closure.contains(d))
                && seen.insert(value.id)
            {
                seeds.push(EvpSeed {
                    value: value.id,
                    sort: packed_type(&value.ty)?,
                    origin: surroundings.admission.origin(value.id),
                    class: u32::try_from(seeds.len()).ok()?,
                });
            }
        }
        if i != ii
            && !inst_results(&block.insts[i])
                .iter()
                .all(|v| surroundings.admission.admits(v.id))
        {
            return None;
        }
    }
    if !seeds.iter().all(|s| surroundings.admission.admits(s.value)) {
        return None;
    }
    let mut value_classes: HashMap<_, _> = seeds.iter().map(|s| (s.value, s.class)).collect();
    let mut nodes = Vec::new();
    for &i in &witnesses {
        let node = before_node(
            &block.insts[i],
            u32::try_from(nodes.len()).ok()?,
            u32::try_from(seeds.len() + nodes.len()).ok()?,
            &value_classes,
        )?;
        value_classes.insert(inst_results(&block.insts[i])[0].id, node.owner);
        nodes.push(node);
    }
    let root_node = nodes.last()?;
    let root_id = root_node.node;
    let mut classes = LocalClasses::new(seeds.len() + nodes.len())?;
    let mut events: Vec<_> = nodes.iter().cloned().map(EvpEvent::Node).collect();
    close_congruences(&nodes, &mut classes, &mut events, 0, &mut Vec::new())?;
    let widths: Vec<_> = seeds
        .iter()
        .map(|s| sort_width(&s.sort))
        .chain(nodes.iter().map(|n| sort_width(&n.sort)))
        .collect::<Option<_>>()?;
    let initial: Vec<_> = seeds
        .iter()
        .map(|s| {
            let w = sort_width(&s.sort)?;
            Some(
                surroundings
                    .seed_facts
                    .of_value(s.value)
                    .filter(|f| f.width() == w)
                    .unwrap_or_else(|| KnownBits::top(w)),
            )
        })
        .collect::<Option<_>>()?;
    let facts = RegionFacts::compute(&nodes, &widths, &initial, &classes)?;
    let root_class = classes.find(root_node.owner)?;
    let fact = facts.of(root_class, &classes)?;
    if fact != KnownBits::constant(1, 1) {
        return None;
    }
    let known = value_classes
        .iter()
        .map(|(&value, &class)| Some((value, facts.of(class, &classes)?)))
        .collect::<Option<_>>()?;
    let rhs_class = classes.push()?;
    let rhs_node = u32::try_from(nodes.len()).ok()?;
    events.push(EvpEvent::Epoch(1));
    events.push(EvpEvent::Node(EvpNode {
        node: rhs_node,
        owner: rhs_class,
        source: EvpNodeSource::Derived,
        operation: "const".to_owned(),
        sort: "bits<1>".to_owned(),
        effect: EvpEffectClass::Pure,
        attributes: vec![("value".to_owned(), 1)],
        children: Vec::new(),
    }));
    let bindings = root_node
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Some((
                (if i == 0 {
                    "a"
                } else if rule.contains("count") {
                    "k"
                } else {
                    "b"
                })
                .to_owned(),
                classes.find(*c)?,
            ))
        })
        .collect::<Option<_>>()?;
    events.push(EvpEvent::Rewrite(EvpRewrite {
        rule: rule.to_owned(),
        lhs_pre: root_class,
        rhs_pre: rhs_class,
        result: root_class.min(rhs_class),
        lhs: EvpProofRef::Node(root_id),
        rhs: EvpProofRef::Node(rhs_node),
        bindings,
        premises: vec![EvpPremise {
            class: root_class,
            mask: 1,
            value: 1,
            digits: 1,
        }],
    }));
    let id = |i| EvpRegionId {
        function: function.name.clone(),
        block: if bi == 0 {
            "entry".to_owned()
        } else {
            format!("b{}", block.id)
        },
        instruction: i as u64,
    };
    Some((
        EvpRegion {
            id: id(ii),
            root_value: result.id,
            witnesses: witnesses.into_iter().map(id).collect(),
            seeds,
            events,
            extractions: vec![EvpExtraction {
                class: root_class,
                choice: EvpExtractionChoice::Node(rhs_node),
            }],
            root_class,
        },
        AppliedRewrite {
            function: function.name.clone(),
            block: block.id,
            result: result.clone(),
            instruction: instruction.clone(),
            facts: HashMap::new(),
            twin: None,
            negations: HashMap::new(),
            known,
            rule: ScalarRewrite::GuardKnownTrue,
            replacement: Replacement::Constant(1),
        },
    ))
}
