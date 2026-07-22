//! SQLite-backed persistence for the WorldModel (Section 10: "Storage:
//! SQLite"). Ordinary infrastructure (Philosophy 6's carve-out) — Zone C.
//!
//! Schema: `nodes` (one row per concept node), `node_labels` (a separate
//! table for the multi-valued label list, indexed for fast token lookup —
//! this is the label half of Section 4's coarse index), and `edges` (one
//! row per relation, `relation` nullable for the untyped-association
//! bootstrapping case described in `edge.rs`).

use crate::confidence::Confidence;
use crate::edge::Edge;
use crate::error::{Result, WorldModelError};
use crate::node::Node;
use crate::spatial::SpatialPosition;
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
            std::fs::create_dir_all(parent).map_err(|e| WorldModelError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// In-memory database — used by tests and anywhere a throwaway
    /// WorldModel is useful (e.g. dry-run tooling later).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                external_identity TEXT,
                position_json TEXT,
                confidence_precision REAL NOT NULL,
                confidence_self_verification_count INTEGER NOT NULL,
                confidence_last_updated_unix INTEGER NOT NULL,
                created_at_unix INTEGER NOT NULL,
                last_touched_unix INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_external_identity ON nodes(external_identity);

            CREATE TABLE IF NOT EXISTS node_labels (
                node_id TEXT NOT NULL,
                label TEXT NOT NULL,
                PRIMARY KEY (node_id, label),
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_node_labels_label ON node_labels(label);

            CREATE TABLE IF NOT EXISTS edges (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                relation TEXT,
                weight REAL NOT NULL,
                confidence_precision REAL NOT NULL,
                confidence_self_verification_count INTEGER NOT NULL,
                confidence_last_updated_unix INTEGER NOT NULL,
                created_at_unix INTEGER NOT NULL,
                last_touched_unix INTEGER NOT NULL,
                FOREIGN KEY (source) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (target) REFERENCES nodes(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
            CREATE INDEX IF NOT EXISTS idx_edges_relation ON edges(relation);
            ",
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Nodes
    // ---------------------------------------------------------------

    pub fn insert_node(&self, node: &Node) -> Result<()> {
        let position_json = position_to_json(&node.position)?;
        self.conn.execute(
            "INSERT INTO nodes (id, external_identity, position_json, confidence_precision, confidence_self_verification_count, confidence_last_updated_unix, created_at_unix, last_touched_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                node.id.to_string(),
                node.external_identity.map(|i| i.to_string()),
                position_json,
                node.confidence.precision,
                node.confidence.self_verification_count,
                node.confidence.last_updated_unix as i64,
                node.created_at_unix as i64,
                node.last_touched_unix as i64,
            ],
        )?;
        self.insert_labels(node.id, &node.labels)?;
        Ok(())
    }

    /// Updates a node's mutable fields (external identity, position,
    /// confidence, last-touched) and adds any labels not already present.
    /// There is no label-removal path in Part 2 — no deletion logic exists
    /// this early (that arrives with memory consolidation in Part 6).
    pub fn update_node(&self, node: &Node) -> Result<()> {
        let position_json = position_to_json(&node.position)?;
        let changed = self.conn.execute(
            "UPDATE nodes SET external_identity = ?2, position_json = ?3, confidence_precision = ?4, confidence_self_verification_count = ?5, confidence_last_updated_unix = ?6, last_touched_unix = ?7
             WHERE id = ?1",
            params![
                node.id.to_string(),
                node.external_identity.map(|i| i.to_string()),
                position_json,
                node.confidence.precision,
                node.confidence.self_verification_count,
                node.confidence.last_updated_unix as i64,
                node.last_touched_unix as i64,
            ],
        )?;
        if changed == 0 {
            return Err(WorldModelError::NodeNotFound(node.id.to_string()));
        }
        self.insert_labels(node.id, &node.labels)?;
        Ok(())
    }

    fn insert_labels(&self, node_id: AlynxisId, labels: &[String]) -> Result<()> {
        for label in labels {
            self.conn.execute(
                "INSERT OR IGNORE INTO node_labels (node_id, label) VALUES (?1, ?2)",
                params![node_id.to_string(), normalize_label(label)],
            )?;
        }
        Ok(())
    }

    pub fn get_node(&self, id: AlynxisId) -> Result<Option<Node>> {
        let row = self
            .conn
            .query_row(
                "SELECT external_identity, position_json, confidence_precision, confidence_self_verification_count, confidence_last_updated_unix, created_at_unix, last_touched_unix
                 FROM nodes WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            external_identity,
            position_json,
            precision,
            self_verification_count,
            confidence_last_updated_unix,
            created_at_unix,
            last_touched_unix,
        )) = row
        else {
            return Ok(None);
        };

        let labels = self.get_labels_for_node(id)?;
        let position = json_to_position(position_json)?;

        Ok(Some(Node {
            id,
            labels,
            external_identity: external_identity.map(|s| parse_id(&s)).transpose()?,
            position,
            confidence: Confidence {
                precision,
                self_verification_count,
                last_updated_unix: confidence_last_updated_unix as u64,
            },
            created_at_unix: created_at_unix as u64,
            last_touched_unix: last_touched_unix as u64,
        }))
    }

    fn get_labels_for_node(&self, id: AlynxisId) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT label FROM node_labels WHERE node_id = ?1")?;
        let rows = stmt.query_map(params![id.to_string()], |row| row.get::<_, String>(0))?;
        let mut labels = Vec::new();
        for r in rows {
            labels.push(r?);
        }
        Ok(labels)
    }

    /// Finds node IDs carrying an exact (normalized) label — the
    /// label-token half of Section 4's coarse index.
    pub fn find_node_ids_by_label(&self, label: &str) -> Result<Vec<AlynxisId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT node_id FROM node_labels WHERE label = ?1")?;
        let rows = stmt.query_map(params![normalize_label(label)], |row| {
            row.get::<_, String>(0)
        })?;
        let mut ids = Vec::new();
        for r in rows {
            ids.push(parse_id(&r?)?);
        }
        Ok(ids)
    }

    /// Finds the node bound to a given external identity (Section 3c —
    /// e.g. Part 1's `AdminIdentity.id`), if one exists.
    pub fn find_node_id_by_external_identity(
        &self,
        external_identity: AlynxisId,
    ) -> Result<Option<AlynxisId>> {
        let result = self
            .conn
            .query_row(
                "SELECT id FROM nodes WHERE external_identity = ?1",
                params![external_identity.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        result.map(|s| parse_id(&s)).transpose()
    }

    pub fn node_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn edge_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    // ---------------------------------------------------------------
    // Edges
    // ---------------------------------------------------------------

    pub fn insert_edge(&self, edge: &Edge) -> Result<()> {
        self.conn.execute(
            "INSERT INTO edges (id, source, target, relation, weight, confidence_precision, confidence_self_verification_count, confidence_last_updated_unix, created_at_unix, last_touched_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                edge.id.to_string(),
                edge.source.to_string(),
                edge.target.to_string(),
                edge.relation.map(|r| r.to_string()),
                edge.weight,
                edge.confidence.precision,
                edge.confidence.self_verification_count,
                edge.confidence.last_updated_unix as i64,
                edge.created_at_unix as i64,
                edge.last_touched_unix as i64,
            ],
        )?;
        Ok(())
    }

    pub fn update_edge(&self, edge: &Edge) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE edges SET weight = ?2, confidence_precision = ?3, confidence_self_verification_count = ?4, confidence_last_updated_unix = ?5, last_touched_unix = ?6
             WHERE id = ?1",
            params![
                edge.id.to_string(),
                edge.weight,
                edge.confidence.precision,
                edge.confidence.self_verification_count,
                edge.confidence.last_updated_unix as i64,
                edge.last_touched_unix as i64,
            ],
        )?;
        if changed == 0 {
            return Err(WorldModelError::EdgeNotFound(edge.id.to_string()));
        }
        Ok(())
    }

    pub fn get_edge(&self, id: AlynxisId) -> Result<Option<Edge>> {
        let mut stmt = self.conn.prepare(EDGE_SELECT_BY_ID)?;
        let result = stmt
            .query_row(params![id.to_string()], row_to_edge)
            .optional()?;
        Ok(result)
    }

    pub fn find_edges_by_source(&self, source: AlynxisId) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(EDGE_SELECT_BY_SOURCE)?;
        let rows = stmt.query_map(params![source.to_string()], row_to_edge)?;
        collect_edges(rows)
    }

    pub fn find_edges_by_target(&self, target: AlynxisId) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(EDGE_SELECT_BY_TARGET)?;
        let rows = stmt.query_map(params![target.to_string()], row_to_edge)?;
        collect_edges(rows)
    }

    /// Every edge whose relation is `relation` — the category-bucket half
    /// of Section 4's coarse index.
    pub fn find_edges_by_relation(&self, relation: AlynxisId) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(EDGE_SELECT_BY_RELATION)?;
        let rows = stmt.query_map(params![relation.to_string()], row_to_edge)?;
        collect_edges(rows)
    }

    /// Looks for an existing edge with this exact (source, target,
    /// relation) triple — used by ingestion to decide whether to reinforce
    /// an existing edge or create a new one.
    pub fn find_edge_by_triple(
        &self,
        source: AlynxisId,
        target: AlynxisId,
        relation: Option<AlynxisId>,
    ) -> Result<Option<Edge>> {
        match relation {
            Some(r) => {
                let mut stmt = self.conn.prepare(EDGE_SELECT_BY_TRIPLE_WITH_RELATION)?;
                Ok(stmt
                    .query_row(
                        params![source.to_string(), target.to_string(), r.to_string()],
                        row_to_edge,
                    )
                    .optional()?)
            }
            None => {
                let mut stmt = self.conn.prepare(EDGE_SELECT_BY_TRIPLE_NULL_RELATION)?;
                Ok(stmt
                    .query_row(params![source.to_string(), target.to_string()], row_to_edge)
                    .optional()?)
            }
        }
    }
}

macro_rules! edge_select {
    ($where:expr) => {
        concat!(
            "SELECT id, source, target, relation, weight, confidence_precision, confidence_self_verification_count, confidence_last_updated_unix, created_at_unix, last_touched_unix FROM edges ",
            $where
        )
    };
}

const EDGE_SELECT_BY_ID: &str = edge_select!("WHERE id = ?1");
const EDGE_SELECT_BY_SOURCE: &str = edge_select!("WHERE source = ?1");
const EDGE_SELECT_BY_TARGET: &str = edge_select!("WHERE target = ?1");
const EDGE_SELECT_BY_RELATION: &str = edge_select!("WHERE relation = ?1");
const EDGE_SELECT_BY_TRIPLE_WITH_RELATION: &str =
    edge_select!("WHERE source = ?1 AND target = ?2 AND relation = ?3");
const EDGE_SELECT_BY_TRIPLE_NULL_RELATION: &str =
    edge_select!("WHERE source = ?1 AND target = ?2 AND relation IS NULL");

fn row_to_edge(row: &Row) -> rusqlite::Result<Edge> {
    let id: String = row.get(0)?;
    let source: String = row.get(1)?;
    let target: String = row.get(2)?;
    let relation: Option<String> = row.get(3)?;
    let weight: f64 = row.get(4)?;
    let precision: f64 = row.get(5)?;
    let self_verification_count: u32 = row.get(6)?;
    let confidence_last_updated_unix: i64 = row.get(7)?;
    let created_at_unix: i64 = row.get(8)?;
    let last_touched_unix: i64 = row.get(9)?;

    Ok(Edge {
        id: parse_id_sql(&id)?,
        source: parse_id_sql(&source)?,
        target: parse_id_sql(&target)?,
        relation: relation.map(|r| parse_id_sql(&r)).transpose()?,
        weight,
        confidence: Confidence {
            precision,
            self_verification_count,
            last_updated_unix: confidence_last_updated_unix as u64,
        },
        created_at_unix: created_at_unix as u64,
        last_touched_unix: last_touched_unix as u64,
    })
}

fn collect_edges(rows: impl Iterator<Item = rusqlite::Result<Edge>>) -> Result<Vec<Edge>> {
    let mut edges = Vec::new();
    for r in rows {
        edges.push(r?);
    }
    Ok(edges)
}

fn normalize_label(label: &str) -> String {
    label.trim().to_lowercase()
}

fn parse_id(s: &str) -> Result<AlynxisId> {
    Uuid::parse_str(s)
        .map(AlynxisId::from_uuid)
        .map_err(|e| WorldModelError::Ingestion(format!("malformed stored UUID {s:?}: {e}")))
}

/// Same parse as `parse_id`, but returning `rusqlite::Result` for use
/// inside row-mapping closures, which must return that error type.
fn parse_id_sql(s: &str) -> rusqlite::Result<AlynxisId> {
    Uuid::parse_str(s).map(AlynxisId::from_uuid).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn position_to_json(position: &Option<SpatialPosition>) -> Result<Option<String>> {
    match position {
        Some(p) => Ok(Some(serde_json::to_string(p)?)),
        None => Ok(None),
    }
}

fn json_to_position(json: Option<String>) -> Result<Option<SpatialPosition>> {
    match json {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    #[test]
    fn insert_and_get_node_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let mut node = Node::new(vec!["dog".into(), "canine".into()]);
        node.position = Some(SpatialPosition::new(vec![1.0, 2.0, 3.0]));
        storage.insert_node(&node).unwrap();

        let loaded = storage.get_node(node.id).unwrap().unwrap();
        assert_eq!(loaded.id, node.id);
        assert!(loaded.has_label("dog"));
        assert!(loaded.has_label("canine"));
        assert_eq!(loaded.position, node.position);
    }

    #[test]
    fn get_nonexistent_node_returns_none() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(storage.get_node(AlynxisId::new()).unwrap().is_none());
    }

    #[test]
    fn find_node_ids_by_label_is_case_and_whitespace_insensitive() {
        let storage = Storage::open_in_memory().unwrap();
        let node = Node::new(vec!["Dog".into()]);
        storage.insert_node(&node).unwrap();

        assert_eq!(
            storage.find_node_ids_by_label("dog").unwrap(),
            vec![node.id]
        );
        assert_eq!(
            storage.find_node_ids_by_label("DOG").unwrap(),
            vec![node.id]
        );
        assert_eq!(
            storage.find_node_ids_by_label("  dog  ").unwrap(),
            vec![node.id]
        );
        assert!(storage.find_node_ids_by_label("cat").unwrap().is_empty());
    }

    #[test]
    fn update_node_adds_new_labels_without_removing_old_ones() {
        let storage = Storage::open_in_memory().unwrap();
        let mut node = Node::new(vec!["dog".into()]);
        storage.insert_node(&node).unwrap();

        node.labels.push("canine".into());
        storage.update_node(&node).unwrap();

        let loaded = storage.get_node(node.id).unwrap().unwrap();
        assert!(loaded.has_label("dog"));
        assert!(loaded.has_label("canine"));
    }

    #[test]
    fn update_nonexistent_node_errors() {
        let storage = Storage::open_in_memory().unwrap();
        let node = Node::new(vec!["ghost".into()]);
        let result = storage.update_node(&node);
        assert!(matches!(result, Err(WorldModelError::NodeNotFound(_))));
    }

    #[test]
    fn insert_and_get_edge_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let a = Node::new(vec!["dog".into()]);
        let b = Node::new(vec!["animal".into()]);
        storage.insert_node(&a).unwrap();
        storage.insert_node(&b).unwrap();

        let edge = Edge::new(a.id, b.id, None);
        storage.insert_edge(&edge).unwrap();

        let loaded = storage.get_edge(edge.id).unwrap().unwrap();
        assert_eq!(loaded.source, a.id);
        assert_eq!(loaded.target, b.id);
        assert_eq!(loaded.relation, None);
    }

    #[test]
    fn find_edges_by_source_target_and_relation() {
        let storage = Storage::open_in_memory().unwrap();
        let dog = Node::new(vec!["dog".into()]);
        let cat = Node::new(vec!["cat".into()]);
        let animal = Node::new(vec!["animal".into()]);
        let is_a = Node::new(vec!["is-a".into()]);
        for n in [&dog, &cat, &animal, &is_a] {
            storage.insert_node(n).unwrap();
        }

        let e1 = Edge::new(dog.id, animal.id, Some(is_a.id));
        let e2 = Edge::new(cat.id, animal.id, Some(is_a.id));
        storage.insert_edge(&e1).unwrap();
        storage.insert_edge(&e2).unwrap();

        assert_eq!(storage.find_edges_by_source(dog.id).unwrap().len(), 1);
        assert_eq!(storage.find_edges_by_target(animal.id).unwrap().len(), 2);
        assert_eq!(storage.find_edges_by_relation(is_a.id).unwrap().len(), 2);
    }

    #[test]
    fn find_edge_by_triple_distinguishes_none_and_some_relation() {
        let storage = Storage::open_in_memory().unwrap();
        let a = Node::new(vec!["a".into()]);
        let b = Node::new(vec!["b".into()]);
        let rel = Node::new(vec!["rel".into()]);
        storage.insert_node(&a).unwrap();
        storage.insert_node(&b).unwrap();
        storage.insert_node(&rel).unwrap();

        let untyped = Edge::new(a.id, b.id, None);
        storage.insert_edge(&untyped).unwrap();

        assert!(storage
            .find_edge_by_triple(a.id, b.id, None)
            .unwrap()
            .is_some());
        assert!(storage
            .find_edge_by_triple(a.id, b.id, Some(rel.id))
            .unwrap()
            .is_none());

        let typed = Edge::new(a.id, b.id, Some(rel.id));
        storage.insert_edge(&typed).unwrap();
        assert!(storage
            .find_edge_by_triple(a.id, b.id, Some(rel.id))
            .unwrap()
            .is_some());
    }

    #[test]
    fn external_identity_lookup_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let mut node = Node::new(Vec::new());
        let identity = AlynxisId::new();
        node.external_identity = Some(identity);
        storage.insert_node(&node).unwrap();

        assert_eq!(
            storage.find_node_id_by_external_identity(identity).unwrap(),
            Some(node.id)
        );
        assert_eq!(
            storage
                .find_node_id_by_external_identity(AlynxisId::new())
                .unwrap(),
            None
        );
    }

    #[test]
    fn persists_across_reopen_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("worldmodel.sqlite");
        let node_id;
        {
            let storage = Storage::open(&db_path).unwrap();
            let node = Node::new(vec!["persistent".into()]);
            node_id = node.id;
            storage.insert_node(&node).unwrap();
        }
        {
            let storage = Storage::open(&db_path).unwrap();
            let loaded = storage.get_node(node_id).unwrap().unwrap();
            assert!(loaded.has_label("persistent"));
        }
    }
}
