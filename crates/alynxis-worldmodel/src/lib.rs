//! Alynxis — Part 2: WorldModel.
//!
//! Concept graph substrate for the rest of the project. See Section 10 of
//! the project brief for Part 2's exact scope, and each module's own doc
//! comment for the design decisions made here — particularly `edge.rs`
//! (why relations are reified as nodes rather than hardcoded), `spatial.rs`
//! (structurally-real spatial representation), `confidence.rs` (Section
//! 2b's minimal Part 2 form), and `ingestion.rs` (the Section 7 bug fix,
//! Zone B).

pub mod confidence;
pub mod edge;
pub mod error;
pub mod index;
pub mod ingestion;
pub mod node;
pub mod spatial;
pub mod storage;
pub mod worldmodel;

pub use confidence::Confidence;
pub use edge::Edge;
pub use error::{Result, WorldModelError};
pub use node::Node;
pub use spatial::SpatialPosition;
pub use storage::Storage;
pub use worldmodel::WorldModel;
