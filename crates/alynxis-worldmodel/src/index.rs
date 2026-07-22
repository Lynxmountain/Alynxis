//! Coarse similarity index (Section 4). Zone C — ordinary infrastructure
//! layered on the SQL indices already in `storage.rs`.
//!
//! No pretrained embeddings exist under blank-slate learning, so
//! "similarity" here means **structural/categorical** similarity via the
//! graph, exactly as Section 4 describes: "A coarse index (e.g., bucketing
//! concepts by dominant category/feature edges) can be used to find the
//! initial activation seed quickly." This is not a disguised vector-search
//! system — it's literal category bucketing.
//!
//! This module is the seed-lookup layer Section 4 explicitly says
//! spreading activation (Part 4, not yet built) will consume: "spreading
//! activation — not the index — is what should produce topical relevance."
//! Nothing here spreads activation; it only finds where to start.

use crate::error::Result;
use crate::storage::Storage;
use alynxis_core::AlynxisId;

/// Finds candidate seed nodes for a text token — the entry point future
/// spreading activation will use to find where in the graph to start.
pub fn seed_nodes_for_token(storage: &Storage, token: &str) -> Result<Vec<AlynxisId>> {
    storage.find_node_ids_by_label(token)
}

/// A node's "dominant category": the (relation, target) of its
/// highest-weight outgoing relation-typed edge. Returns `None` if the node
/// has no outgoing edges with a reified relation yet — untyped (`None`-
/// relation) edges don't count as categorization, since they don't encode
/// which category-like relation is being claimed.
pub fn dominant_category(
    storage: &Storage,
    node_id: AlynxisId,
) -> Result<Option<(AlynxisId, AlynxisId)>> {
    let edges = storage.find_edges_by_source(node_id)?;
    let best = edges
        .into_iter()
        .filter(|e| e.relation.is_some())
        .max_by(|a, b| {
            a.weight
                .partial_cmp(&b.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    Ok(best.map(|e| (e.relation.expect("filtered to Some above"), e.target)))
}

/// Every other node sharing `node_id`'s dominant category — i.e. every
/// node with an edge of the *same relation* pointing at the *same target*.
/// This is the coarse "what else is like this" query. Excludes `node_id`
/// itself. Returns an empty list if `node_id` has no dominant category.
pub fn nodes_in_same_category(storage: &Storage, node_id: AlynxisId) -> Result<Vec<AlynxisId>> {
    let Some((relation, category_target)) = dominant_category(storage, node_id)? else {
        return Ok(Vec::new());
    };
    let edges = storage.find_edges_by_target(category_target)?;
    Ok(edges
        .into_iter()
        .filter(|e| e.relation == Some(relation) && e.source != node_id)
        .map(|e| e.source)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion;

    #[test]
    fn seed_nodes_for_token_finds_exact_label_matches() {
        let storage = Storage::open_in_memory().unwrap();
        let (dog_id, _, _, _) =
            ingestion::ingest_relation(&storage, "dog", None, "animal").unwrap();

        let seeds = seed_nodes_for_token(&storage, "dog").unwrap();
        assert_eq!(seeds, vec![dog_id]);

        assert!(seed_nodes_for_token(&storage, "nonexistent")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn dominant_category_ignores_untyped_edges() {
        let storage = Storage::open_in_memory().unwrap();
        let (dog_id, _, _, _) = ingestion::ingest_relation(&storage, "dog", None, "leash").unwrap();
        // Only an untyped (relation=None) edge exists so far — should not
        // count as a category.
        assert!(dominant_category(&storage, dog_id).unwrap().is_none());

        ingestion::ingest_relation(&storage, "dog", Some("is-a"), "animal").unwrap();
        assert!(dominant_category(&storage, dog_id).unwrap().is_some());
    }

    #[test]
    fn nodes_in_same_category_finds_siblings_via_shared_relation_and_target() {
        let storage = Storage::open_in_memory().unwrap();
        let (dog_id, _, animal_id, _) =
            ingestion::ingest_relation(&storage, "dog", Some("is-a"), "animal").unwrap();
        let (cat_id, _, _, _) =
            ingestion::ingest_relation(&storage, "cat", Some("is-a"), "animal").unwrap();
        // A third, unrelated node categorized differently should not show up.
        ingestion::ingest_relation(&storage, "rock", Some("is-a"), "mineral").unwrap();

        let siblings = nodes_in_same_category(&storage, dog_id).unwrap();
        assert_eq!(siblings, vec![cat_id]);

        let siblings_of_cat = nodes_in_same_category(&storage, cat_id).unwrap();
        assert_eq!(siblings_of_cat, vec![dog_id]);

        // Sanity: animal itself has no outgoing is-a edge, so no category.
        assert!(nodes_in_same_category(&storage, animal_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn different_relations_to_same_target_are_not_considered_same_category() {
        let storage = Storage::open_in_memory().unwrap();
        let (dog_id, _, _, _) =
            ingestion::ingest_relation(&storage, "dog", Some("is-a"), "animal").unwrap();
        // "collar" being merely "associated-with" animal (a different
        // relation) shouldn't make it a categorization sibling of dog.
        ingestion::ingest_relation(&storage, "collar", Some("associated-with"), "animal").unwrap();

        let siblings = nodes_in_same_category(&storage, dog_id).unwrap();
        assert!(siblings.is_empty());
    }
}
