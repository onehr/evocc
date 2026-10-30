use super::*;

impl EqualityState {
    /// Bind a fresh state to the first canonical module and frozen target
    /// layout. No e-node is added and revision zero is observable.
    pub fn for_module(module: &Module) -> Result<Self, String> {
        Self::for_target_module(module, IMPLICIT_TARGET)
    }

    /// Bind the complete text profile, not the host that runs the optimizer.
    /// Both admitted layouts use the same total integer rules; the explicit
    /// target remains part of every module and certificate identity.
    pub fn for_target_module(module: &Module, target: &str) -> Result<Self, String> {
        let platform = TargetPlatform::parse(target)
            .ok_or_else(|| format!("unsupported rewrite target `{target}`"))?;
        Self::for_target_module_with_layout(module, target, &platform.layout())
    }

    pub fn for_target_module_with_layout(
        module: &Module,
        target: &str,
        layout: &TargetDataLayout,
    ) -> Result<Self, String> {
        let platform = TargetPlatform::parse(target)
            .ok_or_else(|| format!("unsupported rewrite target `{target}`"))?;
        if layout.with_short_wchar(false).with_signed_char(false) != platform.layout() {
            return Err("rewrite target and data layout disagree".into());
        }
        validate_module(module)?;
        let canonical_input = render_target_module(module, target);
        let translation_unit_identity = sha256(canonical_input.as_bytes());
        Ok(Self {
            target: target.to_owned(),
            translation_unit_identity,
            current_module_identity: translation_unit_identity,
            target_layout_identity: sha256(layout.platform().evoir_layout_key().as_bytes()),
            canonical_input,
            revision: 0,
            ingested: false,
            graph: Egraph::new(),
            values: HashMap::new(),
            leaves: HashMap::new(),
            operators: HashMap::new(),
            next_op: FIRST_PERSISTENT_OP,
            next_rule: FIRST_INSTANCE_RULE,
            history: Vec::new(),
        })
    }
}
