//! Alynxis — Part 3: Memory Systems.
//!
//! Episodic, procedural, and cold storage tiers behind a unified facade
//! (`MemoryStore`), plus objective timestamp logging on episodes (Section
//! 6a). See Section 10 of the project brief for Part 3's exact scope.
//!
//! **Deliberately decoupled from `alynxis-worldmodel`.** This crate only
//! depends on `alynxis-core` (for `AlynxisId`) — episodes reference
//! WorldModel nodes/edges as opaque IDs, with no cross-crate validation
//! that those IDs actually exist in the graph. This keeps the memory
//! system's own storage lifecycle (which tier holds what, when something
//! gets demoted) independent of the WorldModel's concerns, the same way a
//! database's storage-tiering logic doesn't need to understand the
//! semantic content of the rows it's managing. The caller (currently
//! `alynxis-bin`; eventually the Main Loop, Part 19) is what ties the two
//! together, by passing WorldModel IDs in when recording an episode.

pub mod episode;
pub mod error;
pub mod memory_store;
pub mod procedural;
pub mod storage;
pub mod tiers;

pub use episode::{Episode, MemoryTier};
pub use error::{MemoryError, Result};
pub use memory_store::MemoryStore;
pub use procedural::ProceduralPattern;
pub use storage::Storage;
