// Modules - keep some public for advanced usage
mod array_chain;
mod chain_guard;
pub mod cursor;
mod cursor_direction;
#[doc(hidden)]
pub use cursor_direction::CursorAnchor;
pub mod file;
mod file_iterators;
mod file_mut;
mod file_payload;
pub mod filter;
mod guarded_cell;
mod header_inventory;
#[doc(hidden)]
pub use header_inventory::read_retention_header;
pub mod hash;
pub mod lock;
pub mod mmap;
mod object;
mod object_compression;
mod object_hash;
pub mod offset_array;
pub mod reader;
mod reader_recovery;
pub use reader_recovery::{RecoveryReason, RecoveryStatus};
mod row_view;
pub mod sigbus;
mod value_guard;
pub mod writer;
mod writer_entry_arrays;
mod writer_seal;
mod zstd_stream;
#[doc(hidden)]
pub use object_compression::decompress_zstd_payload as decompress_zstd_data;
#[doc(hidden)]
pub use zstd_stream::copy_zstd_stream;

// Core functionality
pub use file::{
    BucketUtilization, Compression, DEFAULT_COMPRESS_THRESHOLD, DEFAULT_JOURNAL_FILE_MODE,
    JournalFile, JournalFileOptions, MIN_COMPRESS_THRESHOLD, PayloadParts,
    normalize_compress_threshold,
};
#[doc(hidden)]
pub use file_payload::RowPinnedPayload;
pub use reader::JournalReader;
pub use writer::{EntryField, EntryWriteOptions, FieldNamePolicy, JournalWriter, StructuredField};

// Essential types for working with readers
pub use cursor::Location;
pub use offset_array::Direction;

// Advanced filtering (for users who need it)
pub use cursor::JournalCursor;
pub use filter::{FilterExpr, JournalFilter, LogicalOp};

// For FFI compatibility and advanced object manipulation
pub use object::{EntryItemsType, HashableObject, HeaderIncompatibleFlags, JournalState};
#[doc(hidden)]
// Hidden row-view types cross the journal-core/journal crate boundary; keep
// them reachable without making them part of the documented SDK surface.
pub use row_view::{CurrentRowMetadata, CurrentRowPayload, CurrentRowView};

// Re-export commonly needed external types
pub use mmap::{ExperimentalMmapStrategy, Mmap, MmapMut, WindowManagerStats};

// Internal utilities that might be needed
pub use crate::file::hash::journal_hash_data;

// Internal re-exports needed by the crate itself (not part of public API)
pub(crate) use object::*;

// Re-export DataObject for journal-index
pub use object::DataObject;
#[doc(hidden)]
pub use object::{HashTable, JournalHeader};

pub type JournalFileMap = JournalFile<Mmap>;
