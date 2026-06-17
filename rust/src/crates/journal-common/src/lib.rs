//! Common types and utilities shared across journal crates.
//!
//! This crate provides foundational types and utilities used by multiple
//! journal-related crates, avoiding code duplication and circular dependencies.

pub mod collections;
pub mod compat;
pub mod time;

pub use time::{EntryTimestamps, Microseconds, RealtimeClock, Seconds, monotonic_now};

// Re-export collection types for convenience
pub use collections::{HashMap, HashSet, VecDeque};
