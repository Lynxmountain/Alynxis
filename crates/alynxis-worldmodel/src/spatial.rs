//! Spatial representation (Philosophy 3, resolved).
//!
//! Spatial relationships must be "structurally real ... not word-based" —
//! genuine geometric/numeric structure, not a bare learned label like
//! "north of" standing in for it (that would reintroduce the Section 7
//! exact-match bug for spatial queries, just relocated to spatial
//! relations instead of node identity).
//!
//! `SpatialPosition` is a plain N-dimensional real vector rather than
//! something 3D-physical-space-specific, per Philosophy 3's resolution
//! text: the representation should be "chosen so it also generalizes to
//! support Alynxis's broader mathematical and logical reasoning rather
//! than being a narrow, spatial-only bolt-on." The same type can back
//! ordinary 3D physical positions once sensory input exists (Part 7+) and,
//! later, more abstract numeric spaces.
//!
//! Nothing in this file is ever a string. "North of," "inside," "on top
//! of" are never stored anywhere — they are computed on demand from real
//! coordinates via the functions below. No sensory/embodiment input exists
//! yet (that's Part 7, Sensory Gateway), so nothing populates positions in
//! Part 2 — this lays down the representation and basic geometric query
//! primitives for later parts to use.

use crate::error::{Result, WorldModelError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialPosition {
    pub coords: Vec<f64>,
}

impl SpatialPosition {
    pub fn new(coords: Vec<f64>) -> Self {
        Self { coords }
    }

    pub fn dimensions(&self) -> usize {
        self.coords.len()
    }
}

/// Euclidean distance between two positions. Both must have the same
/// dimensionality — comparing a 2D position to a 3D one is a genuine
/// error, not something to silently coerce.
pub fn distance(a: &SpatialPosition, b: &SpatialPosition) -> Result<f64> {
    require_same_dimensions(a, b)?;
    let sum_sq: f64 = a
        .coords
        .iter()
        .zip(b.coords.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum();
    Ok(sum_sq.sqrt())
}

/// The (non-normalized) vector pointing from `a` to `b`. Downstream
/// consumers can normalize or project this however a given query needs
/// (e.g. onto a single axis to answer something like "is B north of A" in
/// a coordinate system where one axis is defined as north-south) — this
/// function only computes the real geometric difference, it never encodes
/// any particular labeled direction.
pub fn direction_vector(a: &SpatialPosition, b: &SpatialPosition) -> Result<Vec<f64>> {
    require_same_dimensions(a, b)?;
    Ok(a.coords
        .iter()
        .zip(b.coords.iter())
        .map(|(x, y)| y - x)
        .collect())
}

/// An axis-aligned bounding region, used for containment queries
/// ("is this position inside that region"). Deliberately the simplest
/// possible real geometric containment primitive — arbitrary-shape
/// containment is a later-part concern if/when it's ever needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingRegion {
    pub min: SpatialPosition,
    pub max: SpatialPosition,
}

impl BoundingRegion {
    pub fn new(min: SpatialPosition, max: SpatialPosition) -> Result<Self> {
        require_same_dimensions(&min, &max)?;
        for (lo, hi) in min.coords.iter().zip(max.coords.iter()) {
            if lo > hi {
                return Err(WorldModelError::InvalidPosition(format!(
                    "BoundingRegion min ({lo}) exceeds max ({hi}) on some axis"
                )));
            }
        }
        Ok(Self { min, max })
    }

    pub fn contains(&self, p: &SpatialPosition) -> Result<bool> {
        require_same_dimensions(&self.min, p)?;
        Ok(self
            .min
            .coords
            .iter()
            .zip(self.max.coords.iter())
            .zip(p.coords.iter())
            .all(|((lo, hi), v)| v >= lo && v <= hi))
    }
}

fn require_same_dimensions(a: &SpatialPosition, b: &SpatialPosition) -> Result<()> {
    if a.dimensions() != b.dimensions() {
        return Err(WorldModelError::InvalidPosition(format!(
            "dimension mismatch: {} vs {}",
            a.dimensions(),
            b.dimensions()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_symmetric_and_correct() {
        let a = SpatialPosition::new(vec![0.0, 0.0]);
        let b = SpatialPosition::new(vec![3.0, 4.0]);
        assert_eq!(distance(&a, &b).unwrap(), 5.0);
        assert_eq!(distance(&b, &a).unwrap(), 5.0);
    }

    #[test]
    fn distance_to_self_is_zero() {
        let a = SpatialPosition::new(vec![1.0, 2.0, 3.0]);
        assert_eq!(distance(&a, &a).unwrap(), 0.0);
    }

    #[test]
    fn dimension_mismatch_is_an_error_not_silently_coerced() {
        let a = SpatialPosition::new(vec![0.0, 0.0]);
        let b = SpatialPosition::new(vec![0.0, 0.0, 0.0]);
        assert!(distance(&a, &b).is_err());
        assert!(direction_vector(&a, &b).is_err());
    }

    #[test]
    fn direction_vector_points_from_a_to_b() {
        let a = SpatialPosition::new(vec![1.0, 1.0]);
        let b = SpatialPosition::new(vec![4.0, 5.0]);
        let dir = direction_vector(&a, &b).unwrap();
        assert_eq!(dir, vec![3.0, 4.0]);
    }

    #[test]
    fn bounding_region_contains_works() {
        let region = BoundingRegion::new(
            SpatialPosition::new(vec![0.0, 0.0]),
            SpatialPosition::new(vec![10.0, 10.0]),
        )
        .unwrap();
        assert!(region
            .contains(&SpatialPosition::new(vec![5.0, 5.0]))
            .unwrap());
        assert!(region
            .contains(&SpatialPosition::new(vec![0.0, 0.0]))
            .unwrap()); // boundary inclusive
        assert!(region
            .contains(&SpatialPosition::new(vec![10.0, 10.0]))
            .unwrap());
        assert!(!region
            .contains(&SpatialPosition::new(vec![11.0, 5.0]))
            .unwrap());
        assert!(!region
            .contains(&SpatialPosition::new(vec![-1.0, 5.0]))
            .unwrap());
    }

    #[test]
    fn bounding_region_rejects_inverted_min_max() {
        let result = BoundingRegion::new(
            SpatialPosition::new(vec![10.0, 0.0]),
            SpatialPosition::new(vec![0.0, 10.0]),
        );
        assert!(result.is_err());
    }
}
