//! Opaque assembly operands and successor edges (Section 3.11).

use evocc_types::{EvoIRType, Region};

use crate::{BlockId, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyOutput {
    pub constraint: String,
    pub name: String,
    pub ty: EvoIRType,
    pub incoming: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyInput {
    pub constraint: String,
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyEdge {
    pub target: BlockId,
    pub arguments: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyLabel {
    pub name: String,
    pub edge: AssemblyEdge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineAssembly {
    pub template: String,
    pub volatile: bool,
    pub outputs: Vec<AssemblyOutput>,
    pub inputs: Vec<AssemblyInput>,
    pub clobbers: Vec<String>,
    pub results: Vec<Value>,
    pub mem_in: Value,
    pub world_in: Value,
    pub mem_out: Value,
    pub world_out: Value,
}

/// Memory constraints carry addresses, including write-only outputs.
#[must_use]
pub fn assembly_memory_constraint(constraint: &str) -> bool {
    matches!(constraint, "Q" | "=Q" | "+Q" | "Qo")
}

/// A lowered local-register operand preserves its physical binding in the
/// quoted constraint. The source vocabulary is unchanged; `@xN` is canonical
/// IR metadata, not another source constraint alternative.
pub fn assembly_register_binding(constraint: &str) -> Result<(&str, Option<u8>), String> {
    let Some((base, register)) = constraint.split_once('@') else {
        return Ok((constraint, None));
    };
    let number = register
        .strip_prefix('x')
        .and_then(|text| text.parse::<u8>().ok());
    let Some(number) = number.filter(|n| matches!(n, 0..=15 | 19..=28)) else {
        return Err(format!(
            "unsupported assembly register binding {constraint}"
        ));
    };
    if register != format!("x{number}")
        || !(matches!(base, "r" | "=r" | "+r" | "=&r" | "+&r")
            || (base.len() == 1 && base.as_bytes()[0].is_ascii_digit()))
    {
        return Err(format!(
            "unsupported assembly register binding {constraint}"
        ));
    }
    Ok((base, Some(number)))
}

impl InlineAssembly {
    #[must_use]
    pub fn results(&self) -> Vec<&Value> {
        self.results
            .iter()
            .chain([&self.mem_out, &self.world_out])
            .collect()
    }

    /// Inputs exclude successor arguments: terminator results are not yet
    /// defined while these uses are checked.
    #[must_use]
    pub fn operands(&self) -> Vec<&Value> {
        self.outputs
            .iter()
            .filter_map(|output| output.incoming.as_ref())
            .chain(self.inputs.iter().map(|input| &input.value))
            .chain([&self.mem_in, &self.world_in])
            .collect()
    }

    pub fn operands_mut(&mut self) -> Vec<&mut Value> {
        self.outputs
            .iter_mut()
            .filter_map(|output| output.incoming.as_mut())
            .chain(self.inputs.iter_mut().map(|input| &mut input.value))
            .chain([&mut self.mem_in, &mut self.world_in])
            .collect()
    }

    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        self.outputs
            .iter_mut()
            .filter_map(|output| output.incoming.as_mut())
            .chain(self.inputs.iter_mut().map(|input| &mut input.value))
            .chain(self.results.iter_mut())
            .chain([
                &mut self.mem_in,
                &mut self.world_in,
                &mut self.mem_out,
                &mut self.world_out,
            ])
            .collect()
    }

    pub fn validate(&self, labels: &[AssemblyLabel]) -> Result<(), String> {
        if self.mem_in.ty != EvoIRType::Mem(Region::All)
            || self.mem_out.ty != EvoIRType::Mem(Region::All)
            || self.world_in.ty != EvoIRType::World
            || self.world_out.ty != EvoIRType::World
        {
            return Err("assembly requires mem<all> and world input/output tokens".into());
        }
        let mut results = self.results.iter();
        let mut names = std::collections::HashSet::new();
        for output in &self.outputs {
            let (constraint, binding) = assembly_register_binding(&output.constraint)?;
            if binding.is_some_and(|reg| {
                self.clobbers
                    .iter()
                    .any(|name| name == &format!("x{reg}") || name == &format!("w{reg}"))
            }) {
                return Err(format!(
                    "assembly register binding {} is clobbered",
                    output.constraint
                ));
            }
            if !matches!(constraint, "=r" | "+r" | "=&r" | "+&r" | "=Q" | "+Q") {
                return Err(format!(
                    "unsupported assembly output constraint {}",
                    output.constraint
                ));
            }
            if !output.name.is_empty() && !names.insert(output.name.as_str()) {
                return Err(format!("duplicate assembly operand name {}", output.name));
            }
            if assembly_memory_constraint(&output.constraint) {
                if !matches!(output.ty, EvoIRType::Ptr(_))
                    || output.incoming.as_ref().map(|value| &value.ty) != Some(&output.ty)
                {
                    return Err(format!(
                        "assembly memory output {} requires an address",
                        output.constraint
                    ));
                }
            } else {
                if !assembly_register_type(&output.ty)
                    || results.next().map(|value| &value.ty) != Some(&output.ty)
                {
                    return Err("assembly register result does not match output type/order".into());
                }
                let incoming = output.incoming.as_ref().map(|value| &value.ty);
                let expected = output.constraint.starts_with('+').then_some(&output.ty);
                if incoming != expected {
                    return Err(format!(
                        "assembly output {} has invalid incoming value",
                        output.constraint
                    ));
                }
            }
        }
        if results.next().is_some() {
            return Err("assembly has extra register results".into());
        }
        for input in &self.inputs {
            let (constraint, binding) = assembly_register_binding(&input.constraint)?;
            if binding.is_some_and(|reg| {
                self.clobbers
                    .iter()
                    .any(|name| name == &format!("x{reg}") || name == &format!("w{reg}"))
            }) {
                return Err(format!(
                    "assembly register binding {} is clobbered",
                    input.constraint
                ));
            }
            if !input.name.is_empty() && !names.insert(input.name.as_str()) {
                return Err(format!("duplicate assembly operand name {}", input.name));
            }
            let valid = match constraint {
                "r" | "rZ" => assembly_register_type(&input.value.ty),
                "Q" | "Qo" => matches!(input.value.ty, EvoIRType::Ptr(_)),
                "p" => matches!(input.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }),
                "I" | "J" | "K" | "L" | "i" | "n" => matches!(input.value.ty, EvoIRType::Bits(_)),
                text if text.len() == 1 && text.as_bytes()[0].is_ascii_digit() => self
                    .outputs
                    .get(usize::from(text.as_bytes()[0] - b'0'))
                    .is_some_and(|output| {
                        !assembly_memory_constraint(&output.constraint)
                            && output.ty == input.value.ty
                    }),
                _ => false,
            };
            if !valid {
                return Err(format!(
                    "unsupported assembly input constraint {} or operand type",
                    input.constraint
                ));
            }
        }
        for clobber in &self.clobbers {
            if !assembly_clobber(clobber) {
                return Err(format!("unsupported assembly clobber {clobber}"));
            }
        }
        let mut label_names = std::collections::HashSet::new();
        for label in labels {
            if label.name.is_empty() || !label_names.insert(label.name.as_str()) {
                return Err("assembly labels must have unique nonempty names".into());
            }
        }
        self.validate_references(labels)
    }

    fn validate_references(&self, labels: &[AssemblyLabel]) -> Result<(), String> {
        let operands = self
            .outputs
            .iter()
            .map(|output| (output.name.as_str(), output.constraint.as_str()))
            .chain(
                self.inputs
                    .iter()
                    .map(|input| (input.name.as_str(), input.constraint.as_str())),
            )
            .collect::<Vec<_>>();
        let bytes = self.template.as_bytes();
        let mut position = 0;
        while position < bytes.len() {
            if bytes[position] != b'%' {
                position += 1;
                continue;
            }
            let start = position;
            position += 1;
            if bytes.get(position) == Some(&b'%') {
                position += 1;
                continue;
            }
            let modifier = bytes
                .get(position)
                .copied()
                .filter(|byte| matches!(byte, b'w' | b'x' | b'l'));
            position += usize::from(modifier.is_some());
            let operand = if bytes.get(position) == Some(&b'[') {
                position += 1;
                let name_start = position;
                while position < bytes.len() && bytes[position] != b']' {
                    position += 1;
                }
                if position == bytes.len() {
                    return Err(format!(
                        "unsupported assembly reference {}",
                        &self.template[start..]
                    ));
                }
                let name = &self.template[name_start..position];
                position += 1;
                if modifier == Some(b'l') {
                    if !labels.iter().any(|label| label.name == name) {
                        return Err(format!("unknown assembly label {name}"));
                    }
                    continue;
                }
                operands
                    .iter()
                    .position(|(candidate, _)| !name.is_empty() && *candidate == name)
            } else if let Some(digit) = bytes.get(position).filter(|byte| byte.is_ascii_digit()) {
                position += 1;
                if modifier == Some(b'l') || bytes.get(position).is_some_and(u8::is_ascii_digit) {
                    return Err(format!(
                        "unsupported assembly reference {}",
                        &self.template[start..]
                    ));
                }
                Some(usize::from(*digit - b'0'))
            } else {
                None
            };
            let Some((_, constraint)) = operand.and_then(|index| operands.get(index)) else {
                return Err(format!(
                    "unsupported assembly reference {}",
                    &self.template[start..]
                ));
            };
            if *constraint == "rZ" && !matches!(modifier, Some(b'w' | b'x')) {
                return Err("assembly rZ reference requires %w or %x".into());
            }
        }
        Ok(())
    }
}

/// Values which the existing general-register assembly constraint can carry.
#[must_use]
pub fn assembly_register_type(ty: &EvoIRType) -> bool {
    matches!(
        ty,
        EvoIRType::Bits(1..=64) | EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
    )
}

#[must_use]
pub fn assembly_clobber(name: &str) -> bool {
    matches!(name, "memory" | "cc")
        || name
            .strip_prefix('x')
            .or_else(|| name.strip_prefix('w'))
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| number <= 30)
}
