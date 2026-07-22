//! Zone B — concept-generalization ingestion (Section 7's bug fix).
//!
//! This is exactly what Section 9 names as Zone B content: "WorldModel
//! learning rules." Once Part 9a's self-modification engine exists,
//! changes to this file are logged and queued for Lynx's review before
//! taking effect (`Config::require_zone_b_review`) — unlike Zone C content
//! (ordinary learned domain knowledge), the *rule* for how concepts get
//! created and linked is exactly the kind of thing Section 9 draws the
//! B/C line around.
//!
//! ## The bug this fixes (Section 7)
//!
//! The original build taught "dog, animal" successfully, but then failed
//! to react to "cat, animal" afterward — the concept-ingestion path was
//! doing exact string/token matching rather than true graph-based
//! generalization, so it only reacted to tokens it had already seen. The
//! fix: when a label is encountered that has never been seen before,
//! ALWAYS create a new concept node for it before attempting to link it to
//! anything else. Never silently skip a novel token. See
//! `find_or_create_node_for_label`'s doc comment for the exact branch this
//! bug lived in.

use crate::edge::Edge;
use crate::error::Result;
use crate::node::Node;
use crate::storage::Storage;
use alynxis_core::AlynxisId;

/// Finds the existing node for `label`, or creates a brand-new one if the
/// label has never been seen before.
///
/// The `None` arm below is the literal Section 7 fix: it must always
/// create a new node for a genuinely novel token, never silently do
/// nothing just because the token wasn't recognized.
pub fn find_or_create_node_for_label(storage: &Storage, label: &str) -> Result<AlynxisId> {
    let existing = storage.find_node_ids_by_label(label)?;
    if let Some(id) = existing.into_iter().next() {
        // Known token — reuse the existing node rather than duplicating.
        let mut node = storage
            .get_node(id)?
            .expect("label index pointed at a node that doesn't exist in storage");
        node.touch();
        storage.update_node(&node)?;
        return Ok(id);
    }

    // Novel token — the Section 7 fix.
    let node = Node::new(vec![label.to_string()]);
    storage.insert_node(&node)?;
    tracing::debug!(label = %label, id = %node.id, "created new concept node for novel token");
    Ok(node.id)
}

/// Ingests a relational statement — "subject [relation] object", e.g.
/// "dog, animal" (Section 7's own example, an implicit/untyped relation)
/// or "dog is-a animal" (an explicit relation label). Subject, relation
/// (if given), and object all go through the identical find-or-create
/// path — relation labels are concepts too (see `edge.rs`'s module doc
/// comment for why relations are reified as nodes rather than stored as
/// bare strings or a hardcoded enum).
///
/// If this exact (subject, relation, object) triple has already been
/// ingested before, the existing edge is reinforced rather than
/// duplicated.
///
/// Returns `(subject_id, relation_id, object_id, edge_id)`.
pub fn ingest_relation(
    storage: &Storage,
    subject_label: &str,
    relation_label: Option<&str>,
    object_label: &str,
) -> Result<(AlynxisId, Option<AlynxisId>, AlynxisId, AlynxisId)> {
    let subject_id = find_or_create_node_for_label(storage, subject_label)?;
    let object_id = find_or_create_node_for_label(storage, object_label)?;
    let relation_id = relation_label
        .map(|r| find_or_create_node_for_label(storage, r))
        .transpose()?;

    let edge_id = match storage.find_edge_by_triple(subject_id, object_id, relation_id)? {
        Some(mut existing) => {
            existing.reinforce();
            storage.update_edge(&existing)?;
            existing.id
        }
        None => {
            let edge = Edge::new(subject_id, object_id, relation_id);
            storage.insert_edge(&edge)?;
            edge.id
        }
    };

    Ok((subject_id, relation_id, object_id, edge_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_storage() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    /// The exact Section 7 regression scenario: teach "dog, animal", then
    /// "cat, animal", and confirm "cat" spawns a genuinely new node rather
    /// than being silently ignored because "cat" is a token the system has
    /// never seen — while "animal" is correctly REUSED, not duplicated,
    /// since it's already known.
    #[test]
    fn section_7_regression_new_token_after_known_token_spawns_new_node() {
        let storage = test_storage();

        let (dog_id, _, animal_id_1, _) = ingest_relation(&storage, "dog", None, "animal").unwrap();
        let (cat_id, _, animal_id_2, _) = ingest_relation(&storage, "cat", None, "animal").unwrap();

        // The critical assertion: cat must be a NEW node, not silently
        // dropped or aliased onto dog or animal.
        assert_ne!(cat_id, dog_id);
        assert_ne!(cat_id, animal_id_1);

        // animal must be the SAME node both times — reused, not duplicated.
        assert_eq!(animal_id_1, animal_id_2);

        let dog_node = storage.get_node(dog_id).unwrap().unwrap();
        let cat_node = storage.get_node(cat_id).unwrap().unwrap();
        assert!(dog_node.has_label("dog"));
        assert!(cat_node.has_label("cat"));
        assert!(!dog_node.has_label("cat"));
    }

    #[test]
    fn repeated_ingestion_reinforces_rather_than_duplicates() {
        let storage = test_storage();

        let (_, _, _, edge_id_1) = ingest_relation(&storage, "dog", None, "animal").unwrap();
        let (_, _, _, edge_id_2) = ingest_relation(&storage, "dog", None, "animal").unwrap();

        assert_eq!(
            edge_id_1, edge_id_2,
            "same triple ingested twice should reinforce the same edge, not create a duplicate"
        );

        let edge = storage.get_edge(edge_id_1).unwrap().unwrap();
        assert!(
            edge.weight > 1.0,
            "repeated ingestion should have increased edge weight"
        );
    }

    #[test]
    fn named_relation_is_reified_as_its_own_concept_node() {
        let storage = test_storage();

        let (subject_id, relation_id, object_id, _) =
            ingest_relation(&storage, "dog", Some("is-a"), "animal").unwrap();

        let relation_id = relation_id.expect("named relation should produce Some(relation_id)");
        let relation_node = storage.get_node(relation_id).unwrap().unwrap();
        assert!(relation_node.has_label("is-a"));
        assert_ne!(relation_id, subject_id);
        assert_ne!(relation_id, object_id);
    }

    #[test]
    fn same_relation_label_reused_across_multiple_triples() {
        let storage = test_storage();

        let (_, relation_1, _, _) =
            ingest_relation(&storage, "dog", Some("is-a"), "animal").unwrap();
        let (_, relation_2, _, _) =
            ingest_relation(&storage, "cat", Some("is-a"), "animal").unwrap();

        assert_eq!(
            relation_1, relation_2,
            "the same relation label should resolve to the same reified relation node, not a fresh one each time"
        );
    }

    #[test]
    fn subject_and_object_can_be_the_same_label_reused_consistently() {
        // Not a normal case, but shouldn't panic or silently misbehave:
        // e.g. "cat, cat" (a self-referential or degenerate statement).
        let storage = test_storage();
        let (subject_id, _, object_id, _) = ingest_relation(&storage, "cat", None, "cat").unwrap();
        assert_eq!(subject_id, object_id);
    }

    /// Section 12's weak-test-coverage lesson: stress this with more than
    /// the minimum number of concepts needed to trivially pass, so a
    /// dedup/generalization regression would actually be caught.
    #[test]
    fn many_distinct_novel_tokens_all_spawn_distinct_nodes() {
        let storage = test_storage();
        let animals = ["dog", "cat", "bird", "fish", "horse", "cow", "sheep", "pig"];
        let mut ids = Vec::new();
        for animal in animals {
            let (subject_id, _, _, _) = ingest_relation(&storage, animal, None, "animal").unwrap();
            ids.push(subject_id);
        }
        let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(
            unique_count,
            animals.len(),
            "every distinct novel token should produce a distinct node"
        );
    }

    #[test]
    fn label_matching_is_case_insensitive_so_dog_and_dog_capitalized_are_the_same_node() {
        let storage = test_storage();
        let (id1, _, _, _) = ingest_relation(&storage, "dog", None, "animal").unwrap();
        let (id2, _, _, _) = ingest_relation(&storage, "Dog", None, "animal").unwrap();
        assert_eq!(id1, id2);
    }
}
