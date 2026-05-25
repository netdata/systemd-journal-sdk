//! Core functionality for working with systemd journal files.
//!
//! This crate provides low-level file I/O for systemd journal files.
//!
//! For related functionality:
//! - High-level journaling with rotation and retention: see `journal-log-writer` crate
//! - File tracking and monitoring: see `journal-registry` crate
//! - Indexing and querying: see `journal-index` crate

// Core error types used throughout the crate
pub mod error;

// Collection type aliases
pub mod collections;

// Low-level journal file format I/O
pub mod file;

// Forward Secure Sealing primitives
// Low-level `journal-core` module for FSS integration.
// The high-level Rust SDK public verification API is not finalized and will come in Phase 2B.
pub mod fss;

// Forward Secure Sealing writer support
pub mod seal;

// Field name mapping for systemd compatibility
pub mod field_map;

// Re-export repository types from journal-registry for convenience
pub mod repository {
    pub use journal_registry::repository::*;
}

// Re-export commonly used types for convenience
pub use error::{JournalError, Result};

// File module re-exports
pub use file::{
    BucketUtilization, Direction, JournalCursor, JournalFile, JournalFileOptions, JournalReader,
    JournalWriter, Location,
};

// SIGBUS handler for memory-mapped file access
pub use file::sigbus::install_handler as install_sigbus_handler;
