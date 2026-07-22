//! Procedural memory (Section 4a, recovered detail via Section 10's Part 3
//! description). Zone C — ordinary data shape.
//!
//! Section 4a's own language for how procedural memory forms is that
//! repeated, similar episodes get "collapsed into a strengthened
//! generalized/procedural **node**" — i.e. procedural memory ultimately
//! manifests as a WorldModel concept node (Part 2), created later by Part
//! 6's consolidation logic. `ProceduralPattern` here is the memory
//! system's own record of that consolidation having happened: which
//! source episodes fed into it, and which WorldModel node now represents
//! the generalized pattern.
//!
//! Nothing populates `source_episode_ids` or `schema_node_id` in Part 3 —
//! no consolidation logic exists yet (Part 6). This is the same kind of
//! honest placeholder Part 2 used for `SpatialPosition` (representation
//! exists, nothing populates it until the part that needs it arrives).

use alynxis_core::AlynxisId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralPattern {
    pub id: AlynxisId,
    /// The WorldModel node representing this generalized pattern, once
    /// Part 6's consolidation has created one. `None` until then.
    pub schema_node_id: Option<AlynxisId>,
    /// Which episodes were consolidated into this pattern. Empty until
    /// Part 6 does the consolidating.
    pub source_episode_ids: Vec<AlynxisId>,
    pub created_at_unix: u64,
    pub last_touched_unix: u64,
}

impl ProceduralPattern {
    pub fn new() -> Self {
        let now = now_unix();
        Self {
            id: AlynxisId::new(),
            schema_node_id: None,
            source_episode_ids: Vec::new(),
            created_at_unix: now,
            last_touched_unix: now,
        }
    }

    pub fn touch(&mut self) {
        self.last_touched_unix = now_unix();
    }
}

impl Default for ProceduralPattern {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pattern_starts_empty_and_unlinked() {
        let p = ProceduralPattern::new();
        assert!(p.schema_node_id.is_none());
        assert!(p.source_episode_ids.is_empty());
    }
}
