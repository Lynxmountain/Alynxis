//! Generic identifier types shared across future parts.
//!
//! Per Section 7a, the WorldModel must not use a fixed `NodeKind` enum for
//! learned structural roles in the concept graph — but that resolution is
//! specifically about learned content, not infrastructure-level
//! identifiers. A UUID newtype here is ordinary plumbing (Philosophy 6) and
//! gives later parts (WorldModel node IDs, agent identity, episodic memory
//! IDs, etc.) a common type to build on rather than each rolling its own.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlynxisId(pub Uuid);

impl AlynxisId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AlynxisId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AlynxisId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_unique() {
        let a = AlynxisId::new();
        let b = AlynxisId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn round_trips_through_uuid() {
        let id = AlynxisId::new();
        let uuid = id.as_uuid();
        let rebuilt = AlynxisId::from_uuid(uuid);
        assert_eq!(id, rebuilt);
    }
}
