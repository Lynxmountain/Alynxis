//! Zone B — memory-system structure rules (Section 9 explicitly names
//! "MemorySystem structure rules" as Zone B content, the same list that
//! names WorldModel learning rules).
//!
//! This file holds the tier-transition mechanics: moving an episode
//! between the episodic and cold tiers. What's implemented here is
//! deliberately mechanical only — the *policy* for when something should
//! be demoted or promoted (storage-pressure thresholds, staleness
//! criteria, retention-based eligibility) is Part 6's job (Memory Decay)
//! and Section 4a's storage-pressure conservation mode, neither of which
//! exist yet. Part 3's job, per Section 10, is only to make sure cold
//! storage exists as a destination — "ties directly to Section 4a's
//! storage-pressure conservation mode, which needs somewhere for demoted
//! data to go."

use crate::episode::MemoryTier;
use crate::error::Result;
use crate::storage::Storage;
use alynxis_core::AlynxisId;

/// Moves an episode to the cold tier. Purely mechanical — no judgment
/// about whether this episode *should* be demoted. Idempotent: demoting
/// an already-cold episode is a harmless no-op re-write.
pub fn demote_to_cold(storage: &Storage, episode_id: AlynxisId) -> Result<()> {
    storage.update_episode_tier(episode_id, MemoryTier::Cold)?;
    tracing::debug!(episode_id = %episode_id, "demoted episode to cold tier");
    Ok(())
}

/// Moves an episode back to the episodic tier. Mechanical, same caveats
/// as `demote_to_cold`. The eventual caller of this (once one exists) is
/// expected to be something like a retrieval event reactivating an old
/// cold-tier memory — not implemented here, since retrieval-triggered
/// promotion is a retrieval-system concern (Part 4, spreading activation)
/// layered on top of this mechanical primitive.
pub fn promote_from_cold(storage: &Storage, episode_id: AlynxisId) -> Result<()> {
    storage.update_episode_tier(episode_id, MemoryTier::Episodic)?;
    tracing::debug!(episode_id = %episode_id, "promoted episode from cold tier");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episode::Episode;

    #[test]
    fn demote_then_promote_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let ep = Episode::new(AlynxisId::new(), vec![], vec![]);
        storage.insert_episode(&ep).unwrap();

        demote_to_cold(&storage, ep.id).unwrap();
        assert_eq!(
            storage.get_episode(ep.id).unwrap().unwrap().tier,
            MemoryTier::Cold
        );

        promote_from_cold(&storage, ep.id).unwrap();
        assert_eq!(
            storage.get_episode(ep.id).unwrap().unwrap().tier,
            MemoryTier::Episodic
        );
    }

    #[test]
    fn demoting_already_cold_episode_is_a_harmless_no_op() {
        let storage = Storage::open_in_memory().unwrap();
        let ep = Episode::new(AlynxisId::new(), vec![], vec![]);
        storage.insert_episode(&ep).unwrap();

        demote_to_cold(&storage, ep.id).unwrap();
        demote_to_cold(&storage, ep.id).unwrap();
        assert_eq!(
            storage.get_episode(ep.id).unwrap().unwrap().tier,
            MemoryTier::Cold
        );
    }

    #[test]
    fn demoting_nonexistent_episode_errors() {
        let storage = Storage::open_in_memory().unwrap();
        let result = demote_to_cold(&storage, AlynxisId::new());
        assert!(result.is_err());
    }
}
