//! Concept graph edges.
//!
//! An edge's `relation` is `Option<AlynxisId>` pointing at *another node*
//! in the same graph that represents the relation itself (e.g. a node that
//! has come to mean "is-a"), rather than a bare string label or a
//! hardcoded `RelationKind` enum. This extends Section 7a's resolution
//! (no hardcoded `NodeKind` taxonomy) to relations: a relation like "is-a"
//! is learned content in exactly the same sense "dog" is learned content
//! (Philosophy 1 — emergent language, no dedicated grammar/relation
//! vocabulary hardcoded in). Reifying it as a node means it goes through
//! the identical ingestion/generalization path as any other concept, can
//! accumulate its own confidence, and can be merged with synonyms later
//! the same way any concept can — nothing about relation-vocabulary is
//! special-cased.
//!
//! `relation: None` represents a raw, untyped association — the most
//! primitive "these two things are linked" — which matters for
//! bootstrapping: the very first associations the system ever forms can't
//! be typed by a relation-concept that doesn't exist yet.

use crate::confidence::Confidence;
use crate::node::now_unix;
use alynxis_core::AlynxisId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: AlynxisId,
    pub source: AlynxisId,
    pub target: AlynxisId,
    /// `Some(node_id)` — the relation reified as a concept node.
    /// `None` — a raw, untyped association (bootstrapping case above).
    pub relation: Option<AlynxisId>,
    /// Edge strength. Consumed later by Section 4's spreading-activation
    /// decay math (not implemented here — Part 2 only stores the value).
    pub weight: f64,
    pub confidence: Confidence,
    pub created_at_unix: u64,
    pub last_touched_unix: u64,
}

impl Edge {
    pub fn new(source: AlynxisId, target: AlynxisId, relation: Option<AlynxisId>) -> Self {
        let now = now_unix();
        Self {
            id: AlynxisId::new(),
            source,
            target,
            relation,
            weight: 1.0,
            confidence: Confidence::new_unverified(),
            created_at_unix: now,
            last_touched_unix: now,
        }
    }

    /// Reinforces an edge that was ingested again (the same subject/
    /// relation/object triple observed a second time) — Section 2b treats
    /// repeated corroboration as a confidence-raising signal, and it's
    /// reasonable for edge weight (raw strength) to track repetition too,
    /// since Section 4's spreading activation will eventually consume
    /// `weight` as a measure of how strong this association is.
    pub fn reinforce(&mut self) {
        self.weight += 1.0;
        self.confidence.record_corroboration();
        self.last_touched_unix = now_unix();
    }

    pub fn touch(&mut self) {
        self.last_touched_unix = now_unix();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_edge_has_untyped_relation_by_default_if_none_given() {
        let e = Edge::new(AlynxisId::new(), AlynxisId::new(), None);
        assert!(e.relation.is_none());
        assert_eq!(e.weight, 1.0);
    }

    #[test]
    fn reinforce_increases_weight_and_confidence() {
        let mut e = Edge::new(AlynxisId::new(), AlynxisId::new(), None);
        let before_weight = e.weight;
        let before_precision = e.confidence.precision;
        e.reinforce();
        assert!(e.weight > before_weight);
        assert!(e.confidence.precision > before_precision);
    }
}
