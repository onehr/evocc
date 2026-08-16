//! Proof-carrying compiler: justifications and independent proof checker

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Type aliases (self-contained; no dependency on evocc-egraph)
// ---------------------------------------------------------------------------
pub type EClassId = u32;
pub type RuleId = u32;
pub type SubstitutionId = u32;
pub type FactProofId = u32;

// ---------------------------------------------------------------------------
// Justification
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Justification {
    Rewrite {
        rule: RuleId,
        subst: SubstitutionId,
        premises: Vec<FactProofId>,
    },
    Congruence {
        lhs_node: u32,
        rhs_node: u32,
    },
    Reflexive,
}

// ---------------------------------------------------------------------------
// ProofEdge
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofEdge {
    pub lhs: EClassId,
    pub rhs: EClassId,
    pub why: Justification,
}

// ---------------------------------------------------------------------------
// ProofDAG
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct ProofDAG {
    pub(crate) edges: Vec<ProofEdge>,
    pub(crate) known_ids: HashSet<EClassId>,
}

impl ProofDAG {
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            known_ids: HashSet::new(),
        }
    }

    /// Add a proof edge.
    ///
    /// Rejects self-loops (`lhs == rhs`). Both `lhs` and `rhs` are tracked in
    /// the internal known-id set so that `validate` can enforce referential
    /// integrity.
    pub fn add_edge(
        &mut self,
        lhs: EClassId,
        rhs: EClassId,
        why: Justification,
    ) -> Result<(), String> {
        if lhs == rhs {
            return Err("self-loop not allowed".to_string());
        }
        self.known_ids.insert(lhs);
        self.known_ids.insert(rhs);
        self.edges.push(ProofEdge { lhs, rhs, why });
        Ok(())
    }

    /// Validate the proof DAG.
    ///
    /// The context-free checker accepts only the empty proof. A non-empty
    /// rewrite needs a rule/substitution registry, and congruence needs an
    /// immutable e-node snapshot; accepting opaque numeric IDs here would
    /// turn a syntactic receipt into a forged semantic proof. Day 4 adds the
    /// bound-context validator that can accept those edges.
    pub fn validate(&self) -> Result<(), String> {
        for edge in &self.edges {
            if !self.known_ids.contains(&edge.lhs) {
                return Err(format!(
                    "edge lhs {} is not in the known eclass-id set",
                    edge.lhs
                ));
            }
            if !self.known_ids.contains(&edge.rhs) {
                return Err(format!(
                    "edge rhs {} is not in the known eclass-id set",
                    edge.rhs
                ));
            }
        }

        if let Some(edge) = self.edges.first() {
            return match &edge.why {
                Justification::Reflexive => Err(format!(
                    "Reflexive cannot justify distinct classes {} and {}",
                    edge.lhs, edge.rhs
                )),
                Justification::Rewrite { .. } => {
                    Err("Rewrite proof has no bound rule/substitution context".into())
                }
                Justification::Congruence { .. } => {
                    Err("Congruence proof has no bound e-node snapshot".into())
                }
            };
        }

        Ok(())
    }
}

impl Default for ProofDAG {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_checker_validates_dag() {
        let empty = ProofDAG::new();
        assert!(empty.validate().is_ok(), "empty proof should pass");

        // Numeric endpoints alone cannot turn Reflexive into a valid proof.
        let mut dag = ProofDAG::new();
        dag.add_edge(1, 2, Justification::Reflexive).unwrap();
        dag.add_edge(2, 3, Justification::Reflexive).unwrap();
        dag.add_edge(1, 3, Justification::Reflexive).unwrap();
        assert!(
            dag.validate().is_err(),
            "context-free Reflexive edges must be rejected"
        );

        // -- edge with target id not in the known-id set --
        let mut dag2 = ProofDAG::new();
        dag2.known_ids.insert(10);
        dag2.known_ids.insert(20);
        // rhs 99 was never registered → validation must fail
        dag2.edges.push(ProofEdge {
            lhs: 10,
            rhs: 99,
            why: Justification::Reflexive,
        });
        assert!(
            dag2.validate().is_err(),
            "edge referencing an unknown target id should be rejected"
        );
    }

    #[test]
    fn rejects_self_loops() {
        let mut dag = ProofDAG::new();
        assert!(dag.add_edge(5, 5, Justification::Reflexive).is_err());
    }

    #[test]
    fn unbound_congruence_is_rejected() {
        let mut dag = ProofDAG::new();
        dag.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 0,
                rhs_node: 3,
            },
        )
        .unwrap();
        assert!(dag.validate().is_err());

        let mut dag2 = ProofDAG::new();
        dag2.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 3,
                rhs_node: 0,
            },
        )
        .unwrap();
        assert!(dag2.validate().is_err());

        let mut dag3 = ProofDAG::new();
        dag3.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 3,
                rhs_node: 4,
            },
        )
        .unwrap();
        assert!(dag3.validate().is_err());
    }

    #[test]
    fn detects_cycle_even_with_disconnected_nodes() {
        let mut dag = ProofDAG::new();
        dag.add_edge(1, 2, Justification::Reflexive).unwrap();
        dag.add_edge(2, 1, Justification::Reflexive).unwrap();
        assert!(dag.validate().is_err());
    }

    #[test]
    fn proof_rewrite_is_replayable() {
        let mut dag = ProofDAG::new();
        let a = 1u32;
        let b = 2u32;
        let c = 3u32;
        // add rewrite edges
        dag.add_edge(a, b, Justification::Reflexive).unwrap();
        dag.add_edge(b, c, Justification::Reflexive).unwrap();
        // No rule/substitution context is attached yet.
        assert!(dag.validate().is_err());
        // Now introduce a cycle
        dag.add_edge(c, a, Justification::Reflexive).unwrap();
        assert!(dag.validate().is_err());
    }
}
