//! Episodic memory (Section 3a, Section 6a). Zone C — ordinary data shape;
//! the tier-transition *rules* live in `tiers.rs` (Zone B), not here.
//!
//! An `Episode` records "this happened to me": a reference to whichever
//! WorldModel nodes/edges were involved, tagged with an objective machine
//! timestamp (Section 6a: "Episodic memory entries get a real machine
//! timestamp attached as raw metadata — this is not 'understanding time,'
//! it's accurate record-keeping Alynxis can consult, analogous to a human
//! checking a clock rather than estimating"). `node_refs`/`edge_refs` are
//! opaque `AlynxisId`s — this crate deliberately doesn't depend on
//! `alynxis-worldmodel`, so nothing here validates that those IDs actually
//! exist in the graph; that decoupling is a deliberate Part 3 design
//! choice (see the crate's module doc comment in `lib.rs`).
//!
//! `experiencer` is normally the WorldModel's self-concept node
//! (Section 3a: "something has to own 'this happened to me'"), passed in
//! by the caller rather than assumed — this crate has no way to know what
//! that ID is on its own, again by design (no dependency on
//! `alynxis-worldmodel`).

use alynxis_core::AlynxisId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Millisecond precision, deliberately more precise than the second-level
/// timestamps used elsewhere in the project (Part 1's admin sessions,
/// Part 2's node/edge bookkeeping). Section 6a frames episodic timestamps
/// specifically as the accuracy ground-truth that a later *subjective*
/// duration estimator (Part 13) gets compared against to produce a
/// prediction-error signal — that comparison deserves finer resolution
/// than ordinary infrastructure bookkeeping needs.
pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Which of the three storage tiers (Section 4a, recovered detail via
/// Section 10's Part 3 description) currently holds this episode. This is
/// a storage-lifecycle distinction — ordinary infrastructure (Philosophy
/// 6's carve-out for "data structures, control flow"), not learned
/// content, so a fixed enum here does not repeat Section 7a's rejected
/// `NodeKind` pattern: which *tier* holds a record is a lifecycle/
/// performance concern the system manages about its own storage, not a
/// semantic category the system learns about the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    /// Recently formed / actively relevant.
    Episodic,
    /// Demoted — low retention priority, moved out of the primary
    /// episodic set under storage pressure (Section 4a's conservation
    /// mode) or simple staleness. Part 3 provides the mechanical move;
    /// nothing decides *when* to move something yet (Part 6's job).
    Cold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: AlynxisId,
    pub experiencer: AlynxisId,
    pub timestamp_ms: u64,
    pub node_refs: Vec<AlynxisId>,
    pub edge_refs: Vec<AlynxisId>,
    pub tier: MemoryTier,
    pub last_touched_unix: u64,
}

impl Episode {
    pub fn new(
        experiencer: AlynxisId,
        node_refs: Vec<AlynxisId>,
        edge_refs: Vec<AlynxisId>,
    ) -> Self {
        Self {
            id: AlynxisId::new(),
            experiencer,
            timestamp_ms: now_millis(),
            node_refs,
            edge_refs,
            tier: MemoryTier::Episodic,
            last_touched_unix: now_unix(),
        }
    }

    pub fn touch(&mut self) {
        self.last_touched_unix = now_unix();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_episode_starts_in_episodic_tier_with_fresh_timestamp() {
        let before = now_millis();
        let ep = Episode::new(AlynxisId::new(), vec![AlynxisId::new()], vec![]);
        let after = now_millis();
        assert_eq!(ep.tier, MemoryTier::Episodic);
        assert!(ep.timestamp_ms >= before && ep.timestamp_ms <= after);
    }

    #[test]
    fn distinct_episodes_get_distinct_ids() {
        let a = Episode::new(AlynxisId::new(), vec![], vec![]);
        let b = Episode::new(AlynxisId::new(), vec![], vec![]);
        assert_ne!(a.id, b.id);
    }
}
