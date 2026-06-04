// Modules - keep some public for advanced usage
pub mod cursor;
pub mod file;
mod file_iterators;
mod file_mut;
mod file_payload;
pub mod filter;
mod guarded_cell;
pub mod hash;
pub mod lock;
pub mod mmap;
mod object;
mod object_compression;
mod object_hash;
pub mod offset_array;
pub mod reader;
pub mod sigbus;
mod value_guard;
pub mod writer;
mod writer_entry_arrays;
mod writer_seal;

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

// Re-export commonly needed external types
pub use mmap::{ExperimentalMmapStrategy, Mmap, MmapMut, WindowManagerStats};

// Internal utilities that might be needed
pub use crate::file::hash::journal_hash_data;

// Internal re-exports needed by the crate itself (not part of public API)
pub(crate) use object::*;

// Re-export DataObject for journal-index
pub use object::DataObject;

pub type JournalFileMap = JournalFile<Mmap>;
