//! Alynxis — Part 4: Value System.
//!
//! Seed values (Section 3's foundational drives), the Section 3 priority
//! formula, satisfaction tracking, and weight evolution. Also includes
//! Section 3f's self-capability-enhancement ceiling and Section 3e's
//! `wellbeing_of_others` floor — see the module doc comments on
//! `value.rs` and `wellbeing.rs` for why those are in scope here rather
//! than deferred to a later part.

pub mod error;
pub mod registry;
pub mod value;
pub mod wellbeing;

pub use error::{Result, ValuesError};
pub use registry::ValueRegistry;
pub use value::{Value, ValueKind};
