//! High-level journal log writer with rotation and retention policies
//!
//! This crate provides a high-level interface for writing to systemd journal files
//! in a directory, with automatic rotation and retention management.
//!
//! ## Usage
//!
//! ```no_run
//! use journal_log_writer::{Config, EntryTimestamps, Log, RotationPolicy, RetentionPolicy};
//! use journal_registry::Origin;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure rotation and retention policies
//! let rotation = RotationPolicy::default()
//!     .with_size_of_journal_file(100 * 1024 * 1024); // 100 MB per file
//!
//! let retention = RetentionPolicy::default()
//!     .with_number_of_journal_files(10); // Keep 10 files max
//!
//! let origin = Origin {
//!     machine_id: Some("00112233445566778899aabbccddeeff".parse()?),
//!     namespace: None,
//!     source: journal_registry::Source::System,
//! };
//!
//! let config = Config::new(origin, rotation, retention)
//!     .with_boot_id("ffeeddccbbaa99887766554433221100".parse()?);
//!
//! // Create a log writer
//! let mut log = Log::new(Path::new("/var/log/myapp"), config)?;
//!
//! // Write entries
//! let entry = [
//!     b"MESSAGE=Hello, journal!" as &[u8],
//!     b"PRIORITY=6",
//! ];
//! let timestamps = EntryTimestamps::default()
//!     .with_entry_realtime_usec(1_700_000_000_000_000)
//!     .with_entry_monotonic_usec(1);
//! log.write_entry_with_timestamps(&entry, timestamps)?;
//! log.sync()?;
//! # Ok(())
//! # }
//! ```

mod error;
mod log;

pub use error::{Result, WriterError};
pub use journal_core::file::{
    Compression, EntryField, EntryWriteOptions, FieldNamePolicy, StructuredField,
};
pub use log::{
    Config, EntryTimestamps, Log, LogArtifactSizer, LogIdentityMode, LogLifecycleEvent,
    LogLifecycleObserver, LogLifecycleReason, LogOpenMode, RetentionPolicy, RotationPolicy,
};
