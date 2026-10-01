//! Canonical EVP1 interchange syntax shared by proof producers and readers.
//!
//! This module owns bytes and structure only. Trusted rules, eligibility,
//! replay, extraction cost and EvoIR reassembly remain reader judgments.

use std::fmt;

/// The interchange version a certificate is written in.
///
/// Section 11.3 "Versions": `evp1` carries no budget line, no epoch markers and
/// no premises, and is validated without a scheduler claim; `evp2` adds the
/// three. A reader that does not implement a version rejects it as malformed
/// rather than guessing, which is why this enum has no catch-all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpVersion {
    Evp1,
    Evp2,
}

impl EvpVersion {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Evp1 => "evp1",
            Self::Evp2 => "evp2",
        }
    }
}

/// One bounded quantity of the scheduler, written consumed `/` bound.
///
/// The order is fixed by Section 11.3 and never the reverse: a reader that took
/// the fields the other way round would accept a transaction that overran its
/// ceiling and refuse one that did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvpQuantity {
    pub consumed: u64,
    pub bound: u64,
}

/// Why saturation stopped (Section 8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpStop {
    Saturated,
    NoNewBest,
    /// A producer stopped by a rule of its own, at neither a profile ceiling
    /// nor a fixpoint. Telemetry like the two above: recording `saturated`
    /// instead would claim a fixpoint the transaction did not reach, and a
    /// checker cannot see the difference.
    Policy,
    Epochs,
    Nodes,
    Fuel,
}

impl EvpStop {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Saturated => "saturated",
            Self::NoNewBest => "no-new-best",
            Self::Policy => "policy",
            Self::Epochs => "epochs",
            Self::Nodes => "nodes",
            Self::Fuel => "fuel",
        }
    }
}

/// The bounded scheduler's record for one transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvpBudget {
    pub nodes: EvpQuantity,
    pub epochs: EvpQuantity,
    pub fuel: EvpQuantity,
    /// The node count the fuel bound derives from. Declared, not re-derived:
    /// Section 11.3 says a producer that overstates it only widens a box the
    /// checker still holds it to.
    pub n: u64,
    pub stop: EvpStop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpCertificate {
    pub version: EvpVersion,
    pub registry_name: String,
    pub registry_digest: String,
    pub input_digest: String,
    /// Present exactly for `evp2`.
    pub budget: Option<EvpBudget>,
    pub regions: Vec<EvpRegion>,
    pub output_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvpRegionId {
    pub function: String,
    pub block: String,
    pub instruction: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpRegion {
    pub id: EvpRegionId,
    pub root_value: u32,
    pub witnesses: Vec<EvpRegionId>,
    pub seeds: Vec<EvpSeed>,
    pub events: Vec<EvpEvent>,
    pub extractions: Vec<EvpExtraction>,
    pub root_class: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpSeedOrigin {
    Input,
    Definition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpSeed {
    pub value: u32,
    pub sort: String,
    pub origin: EvpSeedOrigin,
    pub class: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvpEvent {
    Node(EvpNode),
    Rewrite(EvpRewrite),
    Congruence(EvpCongruence),
    /// An `epoch k` marker. Section 11.3 numbers only nodes, rewrites and
    /// congruences, so a marker carries no event number and does not advance
    /// the shared counter.
    Epoch(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpNodeSource {
    Before,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvpEffectClass {
    Pure,
    Guard,
    Effectful,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpNode {
    pub node: u32,
    pub owner: u32,
    pub source: EvpNodeSource,
    pub operation: String,
    pub sort: String,
    pub effect: EvpEffectClass,
    pub attributes: Vec<(String, u64)>,
    pub children: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpProofRef {
    Class(u32),
    Node(u32),
}

/// One known-bits fact a Tier B rewrite relied on (Section 11.3 "Premises").
///
/// `digits` is the class's width in bits rounded up to a whole hexadecimal
/// digit, and both fields carry exactly that many. It is stored rather than
/// derived so a certificate round-trips byte for byte without the format
/// needing to know the class's sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvpPremise {
    pub class: u32,
    pub mask: u64,
    pub value: u64,
    pub digits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpRewrite {
    pub rule: String,
    pub lhs_pre: u32,
    pub rhs_pre: u32,
    pub result: u32,
    pub lhs: EvpProofRef,
    pub rhs: EvpProofRef,
    pub bindings: Vec<(String, u32)>,
    /// The facts the rule's side condition read, in the order Section 11.6
    /// states for that rule. Empty for every rule of `p0.scalar.v1`.
    pub premises: Vec<EvpPremise>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpCongruence {
    pub lhs_pre: u32,
    pub rhs_pre: u32,
    pub result: u32,
    pub lhs_node: u32,
    pub rhs_node: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvpExtractionChoice {
    Seed(u32),
    Node(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvpExtraction {
    pub class: u32,
    pub choice: EvpExtractionChoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvpParseError(String);

impl EvpParseError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for EvpParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EvpParseError {}

/// Parse the canonical line-oriented EVP1 format.
pub fn parse_evp1(bytes: &[u8]) -> Result<EvpCertificate, EvpParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| EvpParseError::new("EVP1 is not UTF-8"))?;
    if !text.is_ascii()
        || !text.ends_with('\n')
        || text.ends_with("\n\n")
        || text.contains(['\r', '\t', '\0'])
    {
        return Err(EvpParseError::new("EVP1 violates its byte envelope"));
    }
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() < 4 {
        return Err(EvpParseError::new("missing EVP header"));
    }
    let version = match lines[0] {
        "evp1" => EvpVersion::Evp1,
        "evp2" => EvpVersion::Evp2,
        _ => return Err(EvpParseError::new("unimplemented EVP version")),
    };
    let registry = fields(lines[1], 3, "registry")?;
    validate_ident(registry[1])?;
    validate_digest(registry[2])?;
    let input = fields(lines[2], 2, "input")?;
    validate_digest(input[1])?;

    let mut position = 3;
    let budget = if lines
        .get(position)
        .is_some_and(|line| line.starts_with("budget "))
    {
        position += 1;
        Some(parse_budget(lines[position - 1])?)
    } else {
        None
    };
    // Section 11.3 "Versions" is what makes the optional production in the
    // grammar decidable: the budget line belongs to evp2 and to no other
    // version, so its presence is checked against the header rather than left
    // to a reader's taste.
    match (version, budget.is_some()) {
        (EvpVersion::Evp1, true) => {
            return Err(EvpParseError::new("evp1 carries no budget line"));
        }
        (EvpVersion::Evp2, false) => {
            return Err(EvpParseError::new("evp2 requires a budget line"));
        }
        _ => {}
    }
    let mut regions = Vec::new();
    while lines
        .get(position)
        .is_some_and(|line| line.starts_with("region "))
    {
        let (region, consumed) = parse_region(&lines[position..])?;
        regions.push(region);
        position += consumed;
    }
    let output = fields(
        lines
            .get(position)
            .ok_or_else(|| EvpParseError::new("missing EVP1 output record"))?,
        2,
        "output",
    )?;
    validate_digest(output[1])?;
    position += 1;
    if position != lines.len() {
        return Err(EvpParseError::new("records follow EVP1 output"));
    }
    if version == EvpVersion::Evp1
        && regions
            .iter()
            .flat_map(|region: &EvpRegion| &region.events)
            .any(|event| matches!(event, EvpEvent::Epoch(_)))
    {
        return Err(EvpParseError::new("evp1 carries no epoch markers"));
    }
    if version == EvpVersion::Evp1
        && regions
            .iter()
            .flat_map(|region: &EvpRegion| &region.events)
            .any(
                |event| matches!(event, EvpEvent::Rewrite(rewrite) if !rewrite.premises.is_empty()),
            )
    {
        return Err(EvpParseError::new("evp1 carries no premises"));
    }
    let certificate = EvpCertificate {
        version,
        registry_name: registry[1].to_owned(),
        registry_digest: registry[2].to_owned(),
        input_digest: input[1].to_owned(),
        budget,
        regions,
        output_digest: output[1].to_owned(),
    };
    if render_evp1(&certificate).as_bytes() != bytes {
        return Err(EvpParseError::new("EVP certificate is not canonical"));
    }
    Ok(certificate)
}

fn parse_region(lines: &[&str]) -> Result<(EvpRegion, usize), EvpParseError> {
    let header = fields(
        lines
            .first()
            .ok_or_else(|| EvpParseError::new("missing region"))?,
        8,
        "region",
    )?;
    if header[2] != "root" || header[4] != "witnesses" || header[6] != "seeds" {
        return Err(EvpParseError::new("malformed region header"));
    }
    let id = parse_region_id(header[1])?;
    let root_value = parse_value(header[3])?;
    let witness_count = usize_from(header[5])?;
    let seed_count = usize_from(header[7])?;
    let mut position = 1;

    let mut witnesses = Vec::with_capacity(witness_count);
    for expected in 0..witness_count {
        let witness = fields(
            lines
                .get(position)
                .ok_or_else(|| EvpParseError::new("truncated witness manifest"))?,
            3,
            "witness",
        )?;
        if usize_from(witness[1])? != expected {
            return Err(EvpParseError::new("witness positions are not consecutive"));
        }
        witnesses.push(parse_region_id(witness[2])?);
        position += 1;
    }

    let mut seeds = Vec::with_capacity(seed_count);
    for expected in 0..seed_count {
        let seed = fields(
            lines
                .get(position)
                .ok_or_else(|| EvpParseError::new("truncated seed manifest"))?,
            6,
            "seed",
        )?;
        if usize_from(seed[1])? != expected {
            return Err(EvpParseError::new("seed positions are not consecutive"));
        }
        validate_packed_type(seed[3])?;
        seeds.push(EvpSeed {
            value: parse_value(seed[2])?,
            sort: seed[3].to_owned(),
            origin: match seed[4] {
                "input" => EvpSeedOrigin::Input,
                "definition" => EvpSeedOrigin::Definition,
                _ => return Err(EvpParseError::new("unknown seed origin")),
            },
            class: parse_prefixed_u32(seed[5], 'c')?,
        });
        position += 1;
    }

    let mut events = Vec::new();
    let mut event_number = 0;
    let mut previous_epoch: Option<u64> = None;
    while lines.get(position).is_some_and(|line| {
        line.starts_with("node ")
            || line.starts_with("rewrite ")
            || line.starts_with("congruence ")
            || line.starts_with("epoch ")
    }) {
        let line = lines[position];
        if line.starts_with("epoch ") {
            let epoch = canonical_u64(fields(line, 2, "epoch")?[1])?;
            if previous_epoch.is_some_and(|previous| previous >= epoch) {
                return Err(EvpParseError::new("epoch markers are not increasing"));
            }
            previous_epoch = Some(epoch);
            events.push(EvpEvent::Epoch(epoch));
            position += 1;
            continue;
        }
        let (number, event) = if line.starts_with("node ") {
            parse_node(line)?
        } else if line.starts_with("rewrite ") {
            parse_rewrite(line)?
        } else {
            parse_congruence(line)?
        };
        if number != event_number {
            return Err(EvpParseError::new("event numbers are not consecutive"));
        }
        event_number += 1;
        events.push(event);
        position += 1;
    }

    let mut extractions = Vec::new();
    while lines
        .get(position)
        .is_some_and(|line| line.starts_with("extract "))
    {
        let extract = fields(lines[position], 4, "extract")?;
        if usize_from(extract[1])? != extractions.len() {
            return Err(EvpParseError::new(
                "extraction positions are not consecutive",
            ));
        }
        let choice = if let Some(seed) = extract[3].strip_prefix("seed:") {
            EvpExtractionChoice::Seed(u32_from(seed)?)
        } else if let Some(node) = extract[3].strip_prefix("node:n") {
            EvpExtractionChoice::Node(u32_from(node)?)
        } else {
            return Err(EvpParseError::new("unknown extraction choice"));
        };
        extractions.push(EvpExtraction {
            class: parse_prefixed_u32(extract[2], 'c')?,
            choice,
        });
        position += 1;
    }
    if extractions.is_empty() {
        return Err(EvpParseError::new("region has no extraction"));
    }
    let root = fields(
        lines
            .get(position)
            .ok_or_else(|| EvpParseError::new("region has no root"))?,
        2,
        "root",
    )?;
    let root_class = parse_prefixed_u32(root[1], 'c')?;
    position += 1;
    if lines.get(position) != Some(&"endregion") {
        return Err(EvpParseError::new("region has no endregion"));
    }
    position += 1;
    Ok((
        EvpRegion {
            id,
            root_value,
            witnesses,
            seeds,
            events,
            extractions,
            root_class,
        },
        position,
    ))
}

fn parse_node(line: &str) -> Result<(usize, EvpEvent), EvpParseError> {
    let node = fields(line, 10, "node")?;
    let operation = named(node[5], "op")?;
    validate_operation(operation)?;
    let sort = named(node[6], "sort")?;
    validate_packed_type(sort)?;
    Ok((
        usize_from(node[1])?,
        EvpEvent::Node(EvpNode {
            node: parse_prefixed_u32(node[2], 'n')?,
            owner: parse_prefixed_u32(node[3], 'c')?,
            source: match named(node[4], "source")? {
                "before" => EvpNodeSource::Before,
                "derived" => EvpNodeSource::Derived,
                _ => return Err(EvpParseError::new("unknown node source")),
            },
            operation: operation.to_owned(),
            sort: sort.to_owned(),
            effect: match named(node[7], "effect")? {
                "pure" => EvpEffectClass::Pure,
                "guard" => EvpEffectClass::Guard,
                "effectful" => EvpEffectClass::Effectful,
                _ => return Err(EvpParseError::new("unknown EVP1 effect class")),
            },
            attributes: parse_attributes(named(node[8], "attrs")?)?,
            children: parse_id_list(named(node[9], "children")?, 'c')?,
        }),
    ))
}

fn parse_rewrite(line: &str) -> Result<(usize, EvpEvent), EvpParseError> {
    // The premise suffix is optional, so the record has one of two field
    // counts. Splitting on it first keeps the fixed part exact rather than
    // letting a miscounted line parse as a shorter one.
    let (head, premises) = match line.split_once(" premises ") {
        Some((head, tail)) => (head, parse_premises(tail)?),
        None => (line, Vec::new()),
    };
    let rewrite = fields(head, 10, "rewrite")?;
    if rewrite[8] != "bind" {
        return Err(EvpParseError::new("rewrite has no bind field"));
    }
    validate_ident(rewrite[2])?;
    Ok((
        usize_from(rewrite[1])?,
        EvpEvent::Rewrite(EvpRewrite {
            rule: rewrite[2].to_owned(),
            lhs_pre: parse_prefixed_u32(rewrite[3], 'c')?,
            rhs_pre: parse_prefixed_u32(rewrite[4], 'c')?,
            result: parse_prefixed_u32(rewrite[5], 'c')?,
            lhs: parse_proof_ref(rewrite[6])?,
            rhs: parse_proof_ref(rewrite[7])?,
            bindings: parse_bindings(rewrite[9])?,
            premises,
        }),
    ))
}

fn render_premises(premises: &[EvpPremise]) -> String {
    if premises.is_empty() {
        return String::new();
    }
    let body = premises
        .iter()
        .map(|premise| {
            format!(
                "c{}:{:0width$x}:{:0width$x}",
                premise.class,
                premise.mask,
                premise.value,
                width = premise.digits
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(" premises {body}")
}

fn parse_premises(text: &str) -> Result<Vec<EvpPremise>, EvpParseError> {
    if text.is_empty() {
        return Err(EvpParseError::new("empty premise list"));
    }
    text.split(',').map(parse_premise).collect()
}

fn parse_premise(text: &str) -> Result<EvpPremise, EvpParseError> {
    let parts = text.split(':').collect::<Vec<_>>();
    let [class, mask, value] = parts.as_slice() else {
        return Err(EvpParseError::new("a premise is class:mask:value"));
    };
    // Section 11.3: the two fields carry the class's width rounded up to a
    // whole hexadecimal digit, so a length disagreement is malformed rather
    // than a fact about a narrower value.
    if mask.len() != value.len() || mask.is_empty() || mask.len() > 16 {
        return Err(EvpParseError::new("premise fields disagree in length"));
    }
    let mask = parse_hex(mask)?;
    let value = parse_hex(value)?;
    if value & !mask != 0 {
        return Err(EvpParseError::new(
            "premise value has a bit outside its mask",
        ));
    }
    Ok(EvpPremise {
        class: parse_prefixed_u32(class, 'c')?,
        mask,
        value,
        digits: parts[1].len(),
    })
}

fn parse_hex(text: &str) -> Result<u64, EvpParseError> {
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EvpParseError::new("premise field is not lowercase hex"));
    }
    u64::from_str_radix(text, 16).map_err(|_| EvpParseError::new("premise field exceeds u64"))
}

fn parse_congruence(line: &str) -> Result<(usize, EvpEvent), EvpParseError> {
    let congruence = fields(line, 7, "congruence")?;
    Ok((
        usize_from(congruence[1])?,
        EvpEvent::Congruence(EvpCongruence {
            lhs_pre: parse_prefixed_u32(congruence[2], 'c')?,
            rhs_pre: parse_prefixed_u32(congruence[3], 'c')?,
            result: parse_prefixed_u32(congruence[4], 'c')?,
            lhs_node: parse_prefixed_u32(congruence[5], 'n')?,
            rhs_node: parse_prefixed_u32(congruence[6], 'n')?,
        }),
    ))
}

/// Render one certificate in its unique canonical EVP1 byte spelling.
#[must_use]
pub fn render_evp1(certificate: &EvpCertificate) -> String {
    let mut text = format!(
        "{}\nregistry {} {}\ninput {}\n",
        certificate.version.mnemonic(),
        certificate.registry_name,
        certificate.registry_digest,
        certificate.input_digest
    );
    if let Some(budget) = &certificate.budget {
        text.push_str(&format!(
            "budget nodes={} epochs={} fuel={} n={} stop={}\n",
            render_quantity(budget.nodes),
            render_quantity(budget.epochs),
            render_quantity(budget.fuel),
            budget.n,
            budget.stop.mnemonic()
        ));
    }
    for region in &certificate.regions {
        text.push_str(&format!(
            "region {} root %v{} witnesses {} seeds {}\n",
            render_region_id(&region.id),
            region.root_value,
            region.witnesses.len(),
            region.seeds.len()
        ));
        for (index, witness) in region.witnesses.iter().enumerate() {
            text.push_str(&format!("witness {index} {}\n", render_region_id(witness)));
        }
        for (index, seed) in region.seeds.iter().enumerate() {
            text.push_str(&format!(
                "seed {index} %v{} {} {} c{}\n",
                seed.value,
                seed.sort,
                match seed.origin {
                    EvpSeedOrigin::Input => "input",
                    EvpSeedOrigin::Definition => "definition",
                },
                seed.class
            ));
        }
        let mut index = 0_usize;
        for event in &region.events {
            if let EvpEvent::Epoch(epoch) = event {
                text.push_str(&format!("epoch {epoch}\n"));
                continue;
            }
            match event {
                EvpEvent::Node(node) => text.push_str(&format!(
                    "node {index} n{} c{} source={} op={} sort={} effect={} attrs={} children={}\n",
                    node.node,
                    node.owner,
                    match node.source {
                        EvpNodeSource::Before => "before",
                        EvpNodeSource::Derived => "derived",
                    },
                    node.operation,
                    node.sort,
                    match node.effect {
                        EvpEffectClass::Pure => "pure",
                        EvpEffectClass::Guard => "guard",
                        EvpEffectClass::Effectful => "effectful",
                    },
                    render_attributes(&node.attributes),
                    render_ids(&node.children, 'c')
                )),
                EvpEvent::Rewrite(rewrite) => text.push_str(&format!(
                    "rewrite {index} {} c{} c{} c{} {} {} bind {}{}\n",
                    rewrite.rule,
                    rewrite.lhs_pre,
                    rewrite.rhs_pre,
                    rewrite.result,
                    render_proof_ref(rewrite.lhs),
                    render_proof_ref(rewrite.rhs),
                    render_bindings(&rewrite.bindings),
                    render_premises(&rewrite.premises)
                )),
                EvpEvent::Congruence(congruence) => text.push_str(&format!(
                    "congruence {index} c{} c{} c{} n{} n{}\n",
                    congruence.lhs_pre,
                    congruence.rhs_pre,
                    congruence.result,
                    congruence.lhs_node,
                    congruence.rhs_node
                )),
                EvpEvent::Epoch(_) => unreachable!("epoch markers are emitted above"),
            }
            index += 1;
        }
        for (index, extraction) in region.extractions.iter().enumerate() {
            let choice = match extraction.choice {
                EvpExtractionChoice::Seed(seed) => format!("seed:{seed}"),
                EvpExtractionChoice::Node(node) => format!("node:n{node}"),
            };
            text.push_str(&format!("extract {index} c{} {choice}\n", extraction.class));
        }
        text.push_str(&format!("root c{}\nendregion\n", region.root_class));
    }
    text.push_str(&format!("output {}\n", certificate.output_digest));
    text
}

fn render_region_id(id: &EvpRegionId) -> String {
    format!("{}/{}/{}", id.function, id.block, id.instruction)
}

fn render_proof_ref(reference: EvpProofRef) -> String {
    match reference {
        EvpProofRef::Class(class) => format!("class:c{class}"),
        EvpProofRef::Node(node) => format!("node:n{node}"),
    }
}

fn render_ids(ids: &[u32], prefix: char) -> String {
    if ids.is_empty() {
        "-".to_owned()
    } else {
        ids.iter()
            .map(|id| format!("{prefix}{id}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn render_attributes(attributes: &[(String, u64)]) -> String {
    if attributes.is_empty() {
        "-".to_owned()
    } else {
        attributes
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn render_bindings(bindings: &[(String, u32)]) -> String {
    if bindings.is_empty() {
        "-".to_owned()
    } else {
        bindings
            .iter()
            .map(|(name, class)| format!("?{name}:c{class}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn render_quantity(quantity: EvpQuantity) -> String {
    format!("{}/{}", quantity.consumed, quantity.bound)
}

fn parse_quantity(text: &str) -> Result<EvpQuantity, EvpParseError> {
    let (consumed, bound) = text
        .split_once('/')
        .ok_or_else(|| EvpParseError::new("a budget quantity is consumed/bound"))?;
    Ok(EvpQuantity {
        consumed: canonical_u64(consumed)?,
        bound: canonical_u64(bound)?,
    })
}

fn parse_budget(line: &str) -> Result<EvpBudget, EvpParseError> {
    let budget = fields(line, 6, "budget")?;
    Ok(EvpBudget {
        nodes: parse_quantity(named(budget[1], "nodes")?)?,
        epochs: parse_quantity(named(budget[2], "epochs")?)?,
        fuel: parse_quantity(named(budget[3], "fuel")?)?,
        n: canonical_u64(named(budget[4], "n")?)?,
        stop: match named(budget[5], "stop")? {
            "saturated" => EvpStop::Saturated,
            "no-new-best" => EvpStop::NoNewBest,
            "policy" => EvpStop::Policy,
            "epochs" => EvpStop::Epochs,
            "nodes" => EvpStop::Nodes,
            "fuel" => EvpStop::Fuel,
            _ => return Err(EvpParseError::new("unknown stop reason")),
        },
    })
}

fn fields<'a>(line: &'a str, count: usize, record: &str) -> Result<Vec<&'a str>, EvpParseError> {
    let fields = line.split(' ').collect::<Vec<_>>();
    if fields.len() != count || fields[0] != record || fields.iter().any(|field| field.is_empty()) {
        return Err(EvpParseError::new(format!("malformed {record} record")));
    }
    Ok(fields)
}

fn named<'a>(field: &'a str, name: &str) -> Result<&'a str, EvpParseError> {
    field
        .strip_prefix(name)
        .and_then(|value| value.strip_prefix('='))
        .ok_or_else(|| EvpParseError::new(format!("missing {name} field")))
}

fn parse_region_id(text: &str) -> Result<EvpRegionId, EvpParseError> {
    let components = text.split('/').collect::<Vec<_>>();
    if components.len() != 3 {
        return Err(EvpParseError::new("malformed region identity"));
    }
    validate_ident(components[0])?;
    validate_ident(components[1])?;
    Ok(EvpRegionId {
        function: components[0].to_owned(),
        block: components[1].to_owned(),
        instruction: canonical_u64(components[2])?,
    })
}

fn parse_proof_ref(text: &str) -> Result<EvpProofRef, EvpParseError> {
    if let Some(class) = text.strip_prefix("class:c") {
        Ok(EvpProofRef::Class(u32_from(class)?))
    } else if let Some(node) = text.strip_prefix("node:n") {
        Ok(EvpProofRef::Node(u32_from(node)?))
    } else {
        Err(EvpParseError::new("malformed proof reference"))
    }
}

fn parse_bindings(text: &str) -> Result<Vec<(String, u32)>, EvpParseError> {
    if text == "-" {
        return Ok(Vec::new());
    }
    let mut bindings: Vec<(String, u32)> = Vec::new();
    for item in text.split(',') {
        let (name, class) = item
            .strip_prefix('?')
            .and_then(|item| item.split_once(":c"))
            .ok_or_else(|| EvpParseError::new("malformed rewrite binding"))?;
        validate_ident(name)?;
        if bindings
            .last()
            .is_some_and(|(previous, _)| previous.as_str() >= name)
        {
            return Err(EvpParseError::new(
                "rewrite bindings are not strictly sorted",
            ));
        }
        bindings.push((name.to_owned(), u32_from(class)?));
    }
    Ok(bindings)
}

fn parse_attributes(text: &str) -> Result<Vec<(String, u64)>, EvpParseError> {
    if text == "-" {
        return Ok(Vec::new());
    }
    let mut attributes: Vec<(String, u64)> = Vec::new();
    for item in text.split(',') {
        let (name, value) = item
            .split_once('=')
            .ok_or_else(|| EvpParseError::new("malformed node attribute"))?;
        validate_ident(name)?;
        if attributes
            .last()
            .is_some_and(|(previous, _)| previous.as_str() >= name)
        {
            return Err(EvpParseError::new(
                "node attributes are not strictly sorted",
            ));
        }
        attributes.push((name.to_owned(), canonical_u64(value)?));
    }
    Ok(attributes)
}

fn parse_id_list(text: &str, prefix: char) -> Result<Vec<u32>, EvpParseError> {
    if text == "-" {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|item| parse_prefixed_u32(item, prefix))
        .collect()
}

fn parse_prefixed_u32(text: &str, prefix: char) -> Result<u32, EvpParseError> {
    u32_from(
        text.strip_prefix(prefix)
            .ok_or_else(|| EvpParseError::new("malformed prefixed identifier"))?,
    )
}

fn parse_value(text: &str) -> Result<u32, EvpParseError> {
    let text = text
        .strip_prefix("%v")
        .ok_or_else(|| EvpParseError::new("malformed value name"))?;
    u32_from(text)
}

fn usize_from(text: &str) -> Result<usize, EvpParseError> {
    usize::try_from(canonical_u64(text)?).map_err(|_| EvpParseError::new("integer exceeds usize"))
}

fn u32_from(text: &str) -> Result<u32, EvpParseError> {
    u32::try_from(canonical_u64(text)?).map_err(|_| EvpParseError::new("integer exceeds u32"))
}

fn canonical_u64(text: &str) -> Result<u64, EvpParseError> {
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(EvpParseError::new("integer is not canonical decimal"));
    }
    text.parse()
        .map_err(|_| EvpParseError::new("integer exceeds u64"))
}

fn validate_digest(text: &str) -> Result<(), EvpParseError> {
    if text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(EvpParseError::new("digest is not lowercase SHA-256"))
    }
}

fn validate_ident(text: &str) -> Result<(), EvpParseError> {
    let mut bytes = text.bytes();
    let first = bytes
        .next()
        .ok_or_else(|| EvpParseError::new("empty identifier"))?;
    if (!first.is_ascii_alphabetic() && first != b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
    {
        return Err(EvpParseError::new("invalid identifier"));
    }
    Ok(())
}

fn validate_operation(text: &str) -> Result<(), EvpParseError> {
    if matches!(
        text,
        "const"
            | "add"
            | "sub"
            | "mul"
            | "and"
            | "or"
            | "xor"
            | "icmp.eq"
            | "icmp.ne"
            | "icmp.slt"
            | "icmp.sle"
            | "icmp.ult"
            | "icmp.ule"
            // Admitted to the operator table at revision 1.9.44.
            | "shl"
            | "lshr"
            | "ashr"
            | "zext"
            | "sext"
            | "trunc"
            | "divisor_nonzero"
            | "signed_div_rem_no_overflow"
            | "signed_add_defined"
            | "signed_sub_defined"
            | "signed_mul_defined"
            | "shift_count_defined.signed"
            | "shift_count_defined.unsigned"
    ) {
        Ok(())
    } else {
        Err(EvpParseError::new("unknown EVP1 operation"))
    }
}

fn validate_packed_type(text: &str) -> Result<(), EvpParseError> {
    if text.contains(' ') {
        return Err(EvpParseError::new("packed type contains whitespace"));
    }
    let mut cursor = TypeCursor::new(text);
    cursor.ty()?;
    if cursor.position == text.len() {
        Ok(())
    } else {
        Err(EvpParseError::new("trailing packed-type text"))
    }
}

struct TypeCursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> TypeCursor<'a> {
    const fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn at(&self, text: &str) -> bool {
        self.text[self.position..].starts_with(text)
    }

    fn take(&mut self, text: &str) -> Result<(), EvpParseError> {
        if self.at(text) {
            self.position += text.len();
            Ok(())
        } else {
            Err(EvpParseError::new("malformed packed type"))
        }
    }

    fn number(&mut self) -> Result<u64, EvpParseError> {
        let start = self.position;
        while self
            .text
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        canonical_u64(&self.text[start..self.position])
    }

    fn list(&mut self, end: &str) -> Result<(), EvpParseError> {
        if self.at(end) {
            return Ok(());
        }
        loop {
            self.ty()?;
            if self.at(end) {
                return Ok(());
            }
            self.take(",")?;
        }
    }

    fn ty(&mut self) -> Result<(), EvpParseError> {
        if self.at("bits<") {
            self.take("bits<")?;
            if self.number()? == 0 {
                return Err(EvpParseError::new("zero bit width"));
            }
            return self.take(">");
        }
        for scalar in ["f32", "f64", "world", "void"] {
            if self.at(scalar) {
                return self.take(scalar);
            }
        }
        for prefix in ["ptr<", "mem<"] {
            if self.at(prefix) {
                self.take(prefix)?;
                let start = self.position;
                while self
                    .text
                    .as_bytes()
                    .get(self.position)
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                {
                    self.position += 1;
                }
                let name = &self.text[start..self.position];
                let valid = if prefix == "ptr<" {
                    matches!(name, "generic" | "global" | "stack" | "threadlocal")
                } else {
                    matches!(
                        name,
                        "all" | "unknown" | "global" | "stack" | "heap" | "escaped"
                    )
                };
                if !valid {
                    return Err(EvpParseError::new("unknown packed-type qualifier"));
                }
                return self.take(">");
            }
        }
        if self.at("vec<") {
            self.take("vec<")?;
            if self.number()? == 0 {
                return Err(EvpParseError::new("zero vector count"));
            }
            self.take(",")?;
            self.ty()?;
            return self.take(">");
        }
        if self.at("tuple<") {
            self.take("tuple<")?;
            self.list(">")?;
            return self.take(">");
        }
        if self.at("fnptr<(") {
            self.take("fnptr<(")?;
            self.list(")")?;
            self.take(")->(")?;
            self.list(")")?;
            return self.take(")>");
        }
        Err(EvpParseError::new("unknown packed type"))
    }
}
