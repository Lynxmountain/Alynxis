//! `MemoryStore` — the unified facade (Section 4a: "All tiers should sit
//! behind a unified facade: one coherent interface the rest of the system
//! queries, without needing to know which tier currently holds a given
//! memory"). Zone C — ordinary orchestration; the actual tier-transition
//! rule lives in `tiers.rs` (Zone B).

use crate::episode::{Episode, MemoryTier};
use crate::error::Result;
use crate::procedural::ProceduralPattern;
use crate::storage::Storage;
use crate::tiers;
use alynxis_core::AlynxisId;
use std::path::Path;

pub struct MemoryStore {
    storage: Storage,
}

impl MemoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            storage: Storage::open(path)?,
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            storage: Storage::open_in_memory()?,
        })
    }

    // -----------------------------------------------------------
    // Episodic (+ cold — same facade, tier-agnostic by default)
    // -----------------------------------------------------------

    /// Records a new episode — "this happened to me" — tagged with an
    /// objective machine timestamp (Section 6a) and references to whatever
    /// WorldModel nodes/edges were involved. Starts in the episodic tier.
    pub fn record_episode(
        &self,
        experiencer: AlynxisId,
        node_refs: Vec<AlynxisId>,
        edge_refs: Vec<AlynxisId>,
    ) -> Result<Episode> {
        let episode = Episode::new(experiencer, node_refs, edge_refs);
        self.storage.insert_episode(&episode)?;
        Ok(episode)
    }

    pub fn get_episode(&self, id: AlynxisId) -> Result<Option<Episode>> {
        self.storage.get_episode(id)
    }

    /// Most recent episodes for a given experiencer, tier-agnostic — the
    /// unified-facade principle in action: this returns episodic *and*
    /// cold-tier memories without the caller needing to ask separately.
    pub fn recent_episodes(&self, experiencer: AlynxisId, limit: u32) -> Result<Vec<Episode>> {
        self.storage.episodes_for_experiencer(experiencer, limit)
    }

    /// Administrative query for a specific tier — intended for future
    /// consolidation/conservation-mode logic (Part 6), not ordinary
    /// retrieval, which should go through `recent_episodes` instead.
    pub fn episodes_in_tier(&self, tier: MemoryTier) -> Result<Vec<Episode>> {
        self.storage.episodes_in_tier(tier)
    }

    pub fn demote_to_cold(&self, episode_id: AlynxisId) -> Result<()> {
        tiers::demote_to_cold(&self.storage, episode_id)
    }

    pub fn promote_from_cold(&self, episode_id: AlynxisId) -> Result<()> {
        tiers::promote_from_cold(&self.storage, episode_id)
    }

    pub fn episode_count(&self) -> Result<u64> {
        self.storage.episode_count()
    }

    // -----------------------------------------------------------
    // Procedural
    // -----------------------------------------------------------

    pub fn create_procedural_pattern(&self) -> Result<ProceduralPattern> {
        let pattern = ProceduralPattern::new();
        self.storage.insert_pattern(&pattern)?;
        Ok(pattern)
    }

    pub fn get_procedural_pattern(&self, id: AlynxisId) -> Result<Option<ProceduralPattern>> {
        self.storage.get_pattern(id)
    }

    pub fn link_episode_to_pattern(
        &self,
        pattern_id: AlynxisId,
        episode_id: AlynxisId,
    ) -> Result<()> {
        let mut pattern = self.storage.get_pattern(pattern_id)?.ok_or_else(|| {
            crate::error::MemoryError::ProceduralPatternNotFound(pattern_id.to_string())
        })?;
        if !pattern.source_episode_ids.contains(&episode_id) {
            pattern.source_episode_ids.push(episode_id);
        }
        pattern.touch();
        self.storage.update_pattern(&pattern)
    }

    pub fn set_pattern_schema_node(
        &self,
        pattern_id: AlynxisId,
        schema_node_id: AlynxisId,
    ) -> Result<()> {
        let mut pattern = self.storage.get_pattern(pattern_id)?.ok_or_else(|| {
            crate::error::MemoryError::ProceduralPatternNotFound(pattern_id.to_string())
        })?;
        pattern.schema_node_id = Some(schema_node_id);
        pattern.touch();
        self.storage.update_pattern(&pattern)
    }

    pub fn pattern_count(&self) -> Result<u64> {
        self.storage.pattern_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve_episode_through_facade() {
        let store = MemoryStore::open_in_memory().unwrap();
        let experiencer = AlynxisId::new();
        let node = AlynxisId::new();
        let episode = store
            .record_episode(experiencer, vec![node], vec![])
            .unwrap();

        let loaded = store.get_episode(episode.id).unwrap().unwrap();
        assert_eq!(loaded.node_refs, vec![node]);
    }

    #[test]
    fn recent_episodes_returns_across_both_tiers() {
        let store = MemoryStore::open_in_memory().unwrap();
        let experiencer = AlynxisId::new();
        let ep1 = store.record_episode(experiencer, vec![], vec![]).unwrap();
        let ep2 = store.record_episode(experiencer, vec![], vec![]).unwrap();
        store.demote_to_cold(ep1.id).unwrap();

        let recent = store.recent_episodes(experiencer, 10).unwrap();
        let ids: Vec<_> = recent.iter().map(|e| e.id).collect();
        assert!(ids.contains(&ep1.id));
        assert!(ids.contains(&ep2.id));
    }

    #[test]
    fn procedural_pattern_lifecycle_through_facade() {
        let store = MemoryStore::open_in_memory().unwrap();
        let pattern = store.create_procedural_pattern().unwrap();
        assert!(pattern.schema_node_id.is_none());

        let ep = store
            .record_episode(AlynxisId::new(), vec![], vec![])
            .unwrap();
        store.link_episode_to_pattern(pattern.id, ep.id).unwrap();

        let schema_node = AlynxisId::new();
        store
            .set_pattern_schema_node(pattern.id, schema_node)
            .unwrap();

        let loaded = store.get_procedural_pattern(pattern.id).unwrap().unwrap();
        assert_eq!(loaded.source_episode_ids, vec![ep.id]);
        assert_eq!(loaded.schema_node_id, Some(schema_node));
    }

    #[test]
    fn linking_same_episode_twice_does_not_duplicate() {
        let store = MemoryStore::open_in_memory().unwrap();
        let pattern = store.create_procedural_pattern().unwrap();
        let ep = store
            .record_episode(AlynxisId::new(), vec![], vec![])
            .unwrap();

        store.link_episode_to_pattern(pattern.id, ep.id).unwrap();
        store.link_episode_to_pattern(pattern.id, ep.id).unwrap();

        let loaded = store.get_procedural_pattern(pattern.id).unwrap().unwrap();
        assert_eq!(loaded.source_episode_ids.len(), 1);
    }

    #[test]
    fn counts_reflect_recorded_data() {
        let store = MemoryStore::open_in_memory().unwrap();
        store
            .record_episode(AlynxisId::new(), vec![], vec![])
            .unwrap();
        store
            .record_episode(AlynxisId::new(), vec![], vec![])
            .unwrap();
        store.create_procedural_pattern().unwrap();

        assert_eq!(store.episode_count().unwrap(), 2);
        assert_eq!(store.pattern_count().unwrap(), 1);
    }
}
