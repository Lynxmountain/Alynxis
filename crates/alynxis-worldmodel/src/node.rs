//! Concept graph nodes (Section 7a: no hardcoded `NodeKind` taxonomy).
//!
//! A node has no "kind" field of any sort — structural role (is this an
//! agent, a self-anchor, an ordinary concept) comes entirely from learned
//! relational structure and reserved identity/external-identity binding,
//! never from a compile-time tag. See `worldmodel.rs` for how the
//! self-concept node and agent-identity nodes are anchored by reserved
//! deterministic ID / `external_identity` respectively, rather than by
//! type.

use crate::confidence::Confidence;
use alynxis_core::AlynxisId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: AlynxisId,

    /// Zero or more textual labels/synonyms. Zero for a non-linguistic
    /// anchor node (e.g. the self-concept node before it has learned to
    /// associate a name with itself, per Section 3a). More than one once
    /// synonym-merging happens (not implemented yet — later-part concern).
    pub labels: Vec<String>,

    /// Ties this node to an external identity anchor outside the graph —
    /// e.g. Part 1's `AdminIdentity.id` for Lynx's agent-node (Section
    /// 3c). `None` for ordinary concept nodes with no such binding.
    pub external_identity: Option<AlynxisId>,

    /// Optional position in some coordinate space (Philosophy 3). `None`
    /// until a later part with sensory/embodiment input populates it.
    pub position: Option<crate::spatial::SpatialPosition>,

    pub confidence: Confidence,

    pub created_at_unix: u64,
    /// Updated whenever the node is meaningfully touched (new label added,
    /// participates in a new edge, etc.) — raw timestamp only; no decay
    /// math here. Full Ebbinghaus retention is Part 6's job.
    pub last_touched_unix: u64,
}

impl Node {
    /// Creates a brand-new node with a fresh random ID. Most callers
    /// should go through `WorldModel`/`ingestion` rather than constructing
    /// nodes directly, so IDs get properly indexed — this is the low-level
    /// constructor those layers build on.
    pub fn new(labels: Vec<String>) -> Self {
        let now = now_unix();
        Self {
            id: AlynxisId::new(),
            labels,
            external_identity: None,
            position: None,
            confidence: Confidence::new_unverified(),
            created_at_unix: now,
            last_touched_unix: now,
        }
    }

    /// Constructs a node with a specific, caller-chosen ID rather than a
    /// random one — used for the self-concept node's reserved deterministic
    /// ID (Section 3a/7a) and for reconstructing nodes loaded from storage.
    pub fn with_id(id: AlynxisId, labels: Vec<String>) -> Self {
        let now = now_unix();
        Self {
            id,
            labels,
            external_identity: None,
            position: None,
            confidence: Confidence::new_unverified(),
            created_at_unix: now,
            last_touched_unix: now,
        }
    }

    pub fn touch(&mut self) {
        self.last_touched_unix = now_unix();
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_nodes_get_unique_ids() {
        let a = Node::new(vec!["dog".into()]);
        let b = Node::new(vec!["dog".into()]);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn has_label_is_case_insensitive() {
        let n = Node::new(vec!["Dog".into()]);
        assert!(n.has_label("dog"));
        assert!(n.has_label("DOG"));
        assert!(!n.has_label("cat"));
    }

    #[test]
    fn touch_updates_last_touched() {
        let mut n = Node::new(vec!["dog".into()]);
        n.last_touched_unix = 0;
        n.touch();
        assert!(n.last_touched_unix > 0);
    }
}
