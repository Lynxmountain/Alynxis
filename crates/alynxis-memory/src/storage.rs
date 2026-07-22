//! SQLite-backed persistence for episodic and procedural memory. Zone C —
//! ordinary infrastructure. Own database file (`memory.sqlite`), separate
//! from the WorldModel's `worldmodel.sqlite` — see `lib.rs`'s module doc
//! comment for why these stay decoupled.

use crate::episode::{Episode, MemoryTier};
use crate::error::{MemoryError, Result};
use crate::procedural::ProceduralPattern;
use alynxis_core::AlynxisId;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use uuid::Uuid;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MemoryError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS episodes (
                id TEXT PRIMARY KEY,
                experiencer TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                node_refs_json TEXT NOT NULL,
                edge_refs_json TEXT NOT NULL,
                tier TEXT NOT NULL,
                last_touched_unix INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_episodes_experiencer ON episodes(experiencer);
            CREATE INDEX IF NOT EXISTS idx_episodes_tier ON episodes(tier);
            CREATE INDEX IF NOT EXISTS idx_episodes_timestamp ON episodes(timestamp_ms);

            CREATE TABLE IF NOT EXISTS procedural_patterns (
                id TEXT PRIMARY KEY,
                schema_node_id TEXT,
                source_episode_ids_json TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL,
                last_touched_unix INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Episodes
    // ---------------------------------------------------------------

    pub fn insert_episode(&self, ep: &Episode) -> Result<()> {
        self.conn.execute(
            "INSERT INTO episodes (id, experiencer, timestamp_ms, node_refs_json, edge_refs_json, tier, last_touched_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ep.id.to_string(),
                ep.experiencer.to_string(),
                ep.timestamp_ms as i64,
                serde_json::to_string(&ep.node_refs)?,
                serde_json::to_string(&ep.edge_refs)?,
                tier_to_str(ep.tier),
                ep.last_touched_unix as i64,
            ],
        )?;
        Ok(())
    }

    pub fn update_episode_tier(&self, id: AlynxisId, tier: MemoryTier) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE episodes SET tier = ?2, last_touched_unix = ?3 WHERE id = ?1",
            params![id.to_string(), tier_to_str(tier), now_unix() as i64],
        )?;
        if changed == 0 {
            return Err(MemoryError::EpisodeNotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn get_episode(&self, id: AlynxisId) -> Result<Option<Episode>> {
        let mut stmt = self.conn.prepare(EPISODE_SELECT_BY_ID)?;
        stmt.query_row(params![id.to_string()], row_to_episode)
            .optional()
            .map_err(MemoryError::from)?
            .map(Ok)
            .transpose()
    }

    /// Every episode for `experiencer`, most recent first, regardless of
    /// tier — the unified-facade principle: callers shouldn't need to know
    /// which tier currently holds a given memory.
    pub fn episodes_for_experiencer(
        &self,
        experiencer: AlynxisId,
        limit: u32,
    ) -> Result<Vec<Episode>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, experiencer, timestamp_ms, node_refs_json, edge_refs_json, tier, last_touched_unix
             FROM episodes WHERE experiencer = ?1 ORDER BY timestamp_ms DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![experiencer.to_string(), limit], row_to_episode)?;
        collect(rows)
    }

    /// Every episode currently in a given tier — an administrative query
    /// (used by Part 6's future consolidation process to find cold-tier
    /// candidates, or episodic-tier candidates for demotion), not part of
    /// the tier-agnostic facade surface most callers should use.
    pub fn episodes_in_tier(&self, tier: MemoryTier) -> Result<Vec<Episode>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, experiencer, timestamp_ms, node_refs_json, edge_refs_json, tier, last_touched_unix
             FROM episodes WHERE tier = ?1 ORDER BY timestamp_ms ASC",
        )?;
        let rows = stmt.query_map(params![tier_to_str(tier)], row_to_episode)?;
        collect(rows)
    }

    pub fn episode_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    // ---------------------------------------------------------------
    // Procedural patterns
    // ---------------------------------------------------------------

    pub fn insert_pattern(&self, p: &ProceduralPattern) -> Result<()> {
        self.conn.execute(
            "INSERT INTO procedural_patterns (id, schema_node_id, source_episode_ids_json, created_at_unix, last_touched_unix)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                p.id.to_string(),
                p.schema_node_id.map(|i| i.to_string()),
                serde_json::to_string(&p.source_episode_ids)?,
                p.created_at_unix as i64,
                p.last_touched_unix as i64,
            ],
        )?;
        Ok(())
    }

    pub fn update_pattern(&self, p: &ProceduralPattern) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE procedural_patterns SET schema_node_id = ?2, source_episode_ids_json = ?3, last_touched_unix = ?4
             WHERE id = ?1",
            params![
                p.id.to_string(),
                p.schema_node_id.map(|i| i.to_string()),
                serde_json::to_string(&p.source_episode_ids)?,
                p.last_touched_unix as i64,
            ],
        )?;
        if changed == 0 {
            return Err(MemoryError::ProceduralPatternNotFound(p.id.to_string()));
        }
        Ok(())
    }

    pub fn get_pattern(&self, id: AlynxisId) -> Result<Option<ProceduralPattern>> {
        self.conn
            .query_row(
                "SELECT schema_node_id, source_episode_ids_json, created_at_unix, last_touched_unix
                 FROM procedural_patterns WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .map(|(schema_node_id, source_json, created_at, last_touched)| {
                Ok(ProceduralPattern {
                    id,
                    schema_node_id: schema_node_id.map(|s| parse_id(&s)).transpose()?,
                    source_episode_ids: serde_json::from_str(&source_json)?,
                    created_at_unix: created_at as u64,
                    last_touched_unix: last_touched as u64,
                })
            })
            .transpose()
    }

    pub fn pattern_count(&self) -> Result<u64> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM procedural_patterns", [], |row| {
                    row.get(0)
                })?;
        Ok(count as u64)
    }
}

const EPISODE_SELECT_BY_ID: &str = "SELECT id, experiencer, timestamp_ms, node_refs_json, edge_refs_json, tier, last_touched_unix FROM episodes WHERE id = ?1";

fn row_to_episode(row: &Row) -> rusqlite::Result<Episode> {
    let id: String = row.get(0)?;
    let experiencer: String = row.get(1)?;
    let timestamp_ms: i64 = row.get(2)?;
    let node_refs_json: String = row.get(3)?;
    let edge_refs_json: String = row.get(4)?;
    let tier_str: String = row.get(5)?;
    let last_touched_unix: i64 = row.get(6)?;

    let node_refs: Vec<AlynxisId> =
        serde_json::from_str(&node_refs_json).map_err(json_to_sql_err)?;
    let edge_refs: Vec<AlynxisId> =
        serde_json::from_str(&edge_refs_json).map_err(json_to_sql_err)?;
    let tier = tier_from_str(&tier_str).map_err(malformed_to_sql_err)?;

    Ok(Episode {
        id: parse_id_sql(&id)?,
        experiencer: parse_id_sql(&experiencer)?,
        timestamp_ms: timestamp_ms as u64,
        node_refs,
        edge_refs,
        tier,
        last_touched_unix: last_touched_unix as u64,
    })
}

fn collect(rows: impl Iterator<Item = rusqlite::Result<Episode>>) -> Result<Vec<Episode>> {
    let mut episodes = Vec::new();
    for r in rows {
        episodes.push(r?);
    }
    Ok(episodes)
}

fn tier_to_str(tier: MemoryTier) -> &'static str {
    match tier {
        MemoryTier::Episodic => "episodic",
        MemoryTier::Cold => "cold",
    }
}

fn tier_from_str(s: &str) -> Result<MemoryTier> {
    match s {
        "episodic" => Ok(MemoryTier::Episodic),
        "cold" => Ok(MemoryTier::Cold),
        other => Err(MemoryError::Malformed(format!("unknown tier {other:?}"))),
    }
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_id(s: &str) -> Result<AlynxisId> {
    Uuid::parse_str(s)
        .map(AlynxisId::from_uuid)
        .map_err(|e| MemoryError::Malformed(format!("malformed stored UUID {s:?}: {e}")))
}

fn parse_id_sql(s: &str) -> rusqlite::Result<AlynxisId> {
    Uuid::parse_str(s).map(AlynxisId::from_uuid).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn json_to_sql_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

fn malformed_to_sql_err(e: MemoryError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get_episode_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let ep = Episode::new(
            AlynxisId::new(),
            vec![AlynxisId::new(), AlynxisId::new()],
            vec![AlynxisId::new()],
        );
        storage.insert_episode(&ep).unwrap();

        let loaded = storage.get_episode(ep.id).unwrap().unwrap();
        assert_eq!(loaded.id, ep.id);
        assert_eq!(loaded.experiencer, ep.experiencer);
        assert_eq!(loaded.node_refs, ep.node_refs);
        assert_eq!(loaded.edge_refs, ep.edge_refs);
        assert_eq!(loaded.tier, MemoryTier::Episodic);
    }

    #[test]
    fn get_nonexistent_episode_returns_none() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.get_episode(AlynxisId::new()).unwrap().is_none());
    }

    #[test]
    fn update_tier_moves_episode_to_cold_and_back() {
        let storage = Storage::open_in_memory().unwrap();
        let ep = Episode::new(AlynxisId::new(), vec![], vec![]);
        storage.insert_episode(&ep).unwrap();

        storage
            .update_episode_tier(ep.id, MemoryTier::Cold)
            .unwrap();
        assert_eq!(
            storage.get_episode(ep.id).unwrap().unwrap().tier,
            MemoryTier::Cold
        );

        storage
            .update_episode_tier(ep.id, MemoryTier::Episodic)
            .unwrap();
        assert_eq!(
            storage.get_episode(ep.id).unwrap().unwrap().tier,
            MemoryTier::Episodic
        );
    }

    #[test]
    fn update_tier_on_nonexistent_episode_errors() {
        let storage = Storage::open_in_memory().unwrap();
        let result = storage.update_episode_tier(AlynxisId::new(), MemoryTier::Cold);
        assert!(matches!(result, Err(MemoryError::EpisodeNotFound(_))));
    }

    #[test]
    fn episodes_for_experiencer_orders_most_recent_first_and_respects_limit() {
        let storage = Storage::open_in_memory().unwrap();
        let experiencer = AlynxisId::new();
        let mut ids_in_insertion_order = Vec::new();
        for _ in 0..5 {
            let ep = Episode::new(experiencer, vec![], vec![]);
            ids_in_insertion_order.push(ep.id);
            storage.insert_episode(&ep).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        let recent = storage.episodes_for_experiencer(experiencer, 3).unwrap();
        assert_eq!(recent.len(), 3);
        // Most recently inserted should come first.
        assert_eq!(recent[0].id, *ids_in_insertion_order.last().unwrap());
    }

    #[test]
    fn episodes_for_experiencer_includes_cold_tier_per_unified_facade_principle() {
        let storage = Storage::open_in_memory().unwrap();
        let experiencer = AlynxisId::new();
        let ep = Episode::new(experiencer, vec![], vec![]);
        storage.insert_episode(&ep).unwrap();
        storage
            .update_episode_tier(ep.id, MemoryTier::Cold)
            .unwrap();

        let results = storage.episodes_for_experiencer(experiencer, 10).unwrap();
        assert_eq!(
            results.len(),
            1,
            "cold-tier episodes must still be visible through the unified facade query"
        );
    }

    #[test]
    fn episodes_in_tier_filters_correctly() {
        let storage = Storage::open_in_memory().unwrap();
        let experiencer = AlynxisId::new();
        let ep1 = Episode::new(experiencer, vec![], vec![]);
        let ep2 = Episode::new(experiencer, vec![], vec![]);
        storage.insert_episode(&ep1).unwrap();
        storage.insert_episode(&ep2).unwrap();
        storage
            .update_episode_tier(ep2.id, MemoryTier::Cold)
            .unwrap();

        assert_eq!(
            storage
                .episodes_in_tier(MemoryTier::Episodic)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(storage.episodes_in_tier(MemoryTier::Cold).unwrap().len(), 1);
    }

    #[test]
    fn insert_and_get_pattern_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let mut pattern = ProceduralPattern::new();
        pattern.source_episode_ids.push(AlynxisId::new());
        storage.insert_pattern(&pattern).unwrap();

        let loaded = storage.get_pattern(pattern.id).unwrap().unwrap();
        assert_eq!(loaded.source_episode_ids, pattern.source_episode_ids);
        assert!(loaded.schema_node_id.is_none());
    }

    #[test]
    fn update_pattern_sets_schema_node() {
        let storage = Storage::open_in_memory().unwrap();
        let mut pattern = ProceduralPattern::new();
        storage.insert_pattern(&pattern).unwrap();

        let schema_node = AlynxisId::new();
        pattern.schema_node_id = Some(schema_node);
        storage.update_pattern(&pattern).unwrap();

        let loaded = storage.get_pattern(pattern.id).unwrap().unwrap();
        assert_eq!(loaded.schema_node_id, Some(schema_node));
    }

    #[test]
    fn persists_across_reopen_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.sqlite");
        let ep_id;
        {
            let storage = Storage::open(&db_path).unwrap();
            let ep = Episode::new(AlynxisId::new(), vec![], vec![]);
            ep_id = ep.id;
            storage.insert_episode(&ep).unwrap();
        }
        {
            let storage = Storage::open(&db_path).unwrap();
            assert!(storage.get_episode(ep_id).unwrap().is_some());
        }
    }
}
