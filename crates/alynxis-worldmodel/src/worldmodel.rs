//! Top-level `WorldModel` (Section 10, Part 2). Zone C — ordinary
//! orchestration/infrastructure tying storage, the coarse index, and
//! ingestion together; the actual "learning rule" content lives in
//! `ingestion.rs` (Zone B), not here.
//!
//! Bootstraps two things Sections 3a and 3c require as soon as a
//! WorldModel exists:
//!
//! - **The self-concept node (Section 3a).** "A minimal self-concept node
//!   must exist starting at Part 1 — a bare anchor in the WorldModel graph
//!   that first-person episodic memory attaches to." Part 1 explicitly has
//!   no WorldModel yet ("Keep this narrow — it does not need the
//!   WorldModel... yet," Section 10), so a graph node literally cannot
//!   exist until the graph itself does. Read as "starting as soon as the
//!   WorldModel exists" — i.e. now, on every `WorldModel::open`. It's
//!   found by a **reserved deterministic ID** (UUID v5), never by a
//!   `NodeKind::Self` tag, per Section 7a.
//! - **Agent-identity binding (Section 3c).** "Alynxis's WorldModel should
//!   bind a persistent admin/developer identity to Lynx's own agent-node."
//!   `bind_admin_identity` finds-or-creates a node carrying Part 1's
//!   `AdminIdentity.id` as its `external_identity` — again identified by
//!   reserved binding, not a `NodeKind::Agent` tag.

use crate::edge::Edge;
use crate::error::Result;
use crate::index;
use crate::ingestion;
use crate::node::Node;
use crate::storage::Storage;
use alynxis_core::AlynxisId;
use std::path::Path;
use uuid::Uuid;

/// A fixed, arbitrary sentinel UUID used only as the namespace seed for
/// deriving reserved node IDs via UUID v5. The specific value carries no
/// meaning — it's ordinary infrastructure (Philosophy 6's carve-out),
/// analogous to reserving row ID 1 for a sentinel record in a database.
/// Nothing about the self-concept node's *content* is hardcoded here, only
/// its *address* is reserved, which is exactly what Section 7a asks for
/// ("a reserved node identity or relation, not a `NodeKind::Self`
/// variant").
const ALYNXIS_RESERVED_NAMESPACE: Uuid = Uuid::from_u128(1);

fn self_node_reserved_id() -> AlynxisId {
    AlynxisId::from_uuid(Uuid::new_v5(
        &ALYNXIS_RESERVED_NAMESPACE,
        b"alynxis-self-concept-node",
    ))
}

pub struct WorldModel {
    storage: Storage,
}

impl WorldModel {
    pub fn open(path: &Path) -> Result<Self> {
        let storage = Storage::open(path)?;
        let wm = Self { storage };
        wm.bootstrap_self_node()?;
        Ok(wm)
    }

    /// In-memory WorldModel — tests, and any future throwaway/dry-run use.
    pub fn open_in_memory() -> Result<Self> {
        let storage = Storage::open_in_memory()?;
        let wm = Self { storage };
        wm.bootstrap_self_node()?;
        Ok(wm)
    }

    fn bootstrap_self_node(&self) -> Result<()> {
        let id = self_node_reserved_id();
        if self.storage.get_node(id)?.is_none() {
            // Empty labels: the self-node starts as a bare, undifferentiated
            // anchor (Section 3a). It has not yet learned to associate any
            // name — including "Alynxis" — with itself; that association
            // is learned the same way any other word-to-referent link is
            // learned, not seeded here.
            let node = Node::with_id(id, Vec::new());
            self.storage.insert_node(&node)?;
            tracing::info!(id = %id, "bootstrapped self-concept node (Section 3a)");
        }
        Ok(())
    }

    /// The self-concept node's reserved ID. Stable across restarts and
    /// across fresh databases — it's derived deterministically, not
    /// randomly generated, so it never needs to be looked up by any
    /// out-of-band mechanism.
    pub fn self_node_id(&self) -> AlynxisId {
        self_node_reserved_id()
    }

    /// Finds-or-creates the agent-node bound to `admin_identity_id` (Part
    /// 1's `AdminIdentity.id`), per Section 3c. Idempotent — calling this
    /// again with the same identity returns the same node.
    pub fn bind_admin_identity(&self, admin_identity_id: AlynxisId) -> Result<AlynxisId> {
        if let Some(existing) = self
            .storage
            .find_node_id_by_external_identity(admin_identity_id)?
        {
            return Ok(existing);
        }
        let mut node = Node::new(Vec::new());
        node.external_identity = Some(admin_identity_id);
        self.storage.insert_node(&node)?;
        tracing::info!(
            node_id = %node.id,
            admin_identity = %admin_identity_id,
            "bound admin identity to new agent-node"
        );
        Ok(node.id)
    }

    // -----------------------------------------------------------
    // Thin pass-throughs to the ingestion/index/storage layers —
    // the public surface most callers should use rather than reaching
    // into `storage`/`ingestion`/`index` directly.
    // -----------------------------------------------------------

    pub fn ingest_relation(
        &self,
        subject_label: &str,
        relation_label: Option<&str>,
        object_label: &str,
    ) -> Result<(AlynxisId, Option<AlynxisId>, AlynxisId, AlynxisId)> {
        ingestion::ingest_relation(&self.storage, subject_label, relation_label, object_label)
    }

    pub fn get_node(&self, id: AlynxisId) -> Result<Option<Node>> {
        self.storage.get_node(id)
    }

    pub fn get_edge(&self, id: AlynxisId) -> Result<Option<Edge>> {
        self.storage.get_edge(id)
    }

    pub fn seed_nodes_for_token(&self, token: &str) -> Result<Vec<AlynxisId>> {
        index::seed_nodes_for_token(&self.storage, token)
    }

    pub fn nodes_in_same_category(&self, node_id: AlynxisId) -> Result<Vec<AlynxisId>> {
        index::nodes_in_same_category(&self.storage, node_id)
    }

    /// Total node count — used by the CLI status report and by tests.
    pub fn node_count(&self) -> Result<u64> {
        self.storage.node_count()
    }

    pub fn edge_count(&self) -> Result<u64> {
        self.storage.edge_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_node_bootstraps_on_open_and_is_idempotent() {
        let wm = WorldModel::open_in_memory().unwrap();
        let id = wm.self_node_id();
        assert!(wm.get_node(id).unwrap().is_some());

        // Bootstrapping again (e.g. simulating a second open against the
        // same underlying storage) shouldn't error or duplicate.
        wm.bootstrap_self_node().unwrap();
        assert_eq!(wm.node_count().unwrap(), 1);
    }

    #[test]
    fn self_node_id_is_deterministic_across_separate_worldmodels() {
        let wm1 = WorldModel::open_in_memory().unwrap();
        let wm2 = WorldModel::open_in_memory().unwrap();
        assert_eq!(wm1.self_node_id(), wm2.self_node_id());
    }

    #[test]
    fn self_node_survives_reopen_on_disk_without_duplication() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("worldmodel.sqlite");
        {
            let wm = WorldModel::open(&db_path).unwrap();
            assert_eq!(wm.node_count().unwrap(), 1);
        }
        {
            let wm = WorldModel::open(&db_path).unwrap();
            // Re-opening should recognize the existing self-node rather
            // than creating a second one.
            assert_eq!(wm.node_count().unwrap(), 1);
        }
    }

    #[test]
    fn bind_admin_identity_is_idempotent() {
        let wm = WorldModel::open_in_memory().unwrap();
        let admin_id = AlynxisId::new();

        let node_id_1 = wm.bind_admin_identity(admin_id).unwrap();
        let node_id_2 = wm.bind_admin_identity(admin_id).unwrap();
        assert_eq!(node_id_1, node_id_2);

        let node = wm.get_node(node_id_1).unwrap().unwrap();
        assert_eq!(node.external_identity, Some(admin_id));
    }

    #[test]
    fn bind_admin_identity_distinguishes_different_identities() {
        let wm = WorldModel::open_in_memory().unwrap();
        let admin_a = AlynxisId::new();
        let admin_b = AlynxisId::new();

        let node_a = wm.bind_admin_identity(admin_a).unwrap();
        let node_b = wm.bind_admin_identity(admin_b).unwrap();
        assert_ne!(node_a, node_b);
    }

    #[test]
    fn end_to_end_ingest_and_query_through_public_api() {
        let wm = WorldModel::open_in_memory().unwrap();
        let (dog_id, _, animal_id, _) = wm.ingest_relation("dog", Some("is-a"), "animal").unwrap();
        let (cat_id, _, _, _) = wm.ingest_relation("cat", Some("is-a"), "animal").unwrap();

        assert_eq!(wm.seed_nodes_for_token("dog").unwrap(), vec![dog_id]);
        assert_eq!(wm.nodes_in_same_category(dog_id).unwrap(), vec![cat_id]);
        assert!(wm.get_node(animal_id).unwrap().is_some());

        // Self-node (1) + dog + is-a + animal + cat = 5 nodes.
        assert_eq!(wm.node_count().unwrap(), 5);
    }
}
