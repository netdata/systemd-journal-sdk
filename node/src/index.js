// Node.js SDK for systemd journal file reading and writing.
// No native addons, pure JavaScript with Buffer/Uint8Array for binary values.

import { FileReader } from './lib/reader.js';
import { DirectoryReader } from './lib/directory-reader.js';
import {
  Direction,
  ExplorerAnchor,
  ExplorerAnchorKind,
  ExplorerComparison,
  ExplorerControl,
  ExplorerError,
  ExplorerFieldMode,
  ExplorerFilter,
  ExplorerFtsPattern,
  ExplorerHistogram,
  ExplorerHistogramBucket,
  ExplorerProgress,
  ExplorerQuery,
  ExplorerResult,
  ExplorerRow,
  ExplorerSampling,
  ExplorerStats,
  ExplorerStopReason,
  ExplorerStrategy,
  ExplorerUnsupported,
  UNSET_VALUE,
  DEFAULT_HISTOGRAM_TARGET_BUCKETS,
  DEFAULT_TIME_SLACK_USEC,
  EXPLORER_CONTROL_CHECK_EVERY_ROWS,
  EXPLORER_PROGRESS_INTERVAL_MS,
} from './lib/explorer.js';
import {
  FIELD_NAME_POLICY_JOURNALD, FIELD_NAME_POLICY_JOURNAL_APP,
  FIELD_NAME_POLICY_RAW, Writer,
} from './lib/writer.js';
import {
  Log,
  LOG_OPEN_LAZY, LOG_OPEN_EAGER,
  LOG_IDENTITY_AUTO, LOG_IDENTITY_STRICT,
  LOG_LIFECYCLE_CREATED, LOG_LIFECYCLE_ROTATED, LOG_LIFECYCLE_DELETED,
  LOG_LIFECYCLE_REASON_APPEND, LOG_LIFECYCLE_REASON_EAGER_OPEN,
  LOG_LIFECYCLE_REASON_ROTATION, LOG_LIFECYCLE_REASON_RETENTION,
} from './lib/directory-writer.js';
import {
  SdJournal, SdJournalOpen, SdJournalOpenDirectory,
} from './facade.js';
import {
  NetdataJournalFunction,
  NetdataRequest,
  CombinedResult,
  JournalFileCollection,
} from './lib/netdata.js';
import {
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_LIVE,
  READER_BOUNDS_SNAPSHOT,
  DEFAULT_WINDOW_SIZE_BYTES,
  DEFAULT_MAX_WINDOWS,
  DEFAULT_MAX_ROW_ARENA_BYTES,
  DEFAULT_ROW_ARENA_SEGMENT_BYTES,
  UnsupportedAccessModeError,
} from './lib/reader-access.js';

// Re-export everything
export { FileReader, DirectoryReader, Writer, Log };
export { WriterLock } from './lib/lock.js';
export {
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_LIVE,
  READER_BOUNDS_SNAPSHOT,
  DEFAULT_WINDOW_SIZE_BYTES,
  DEFAULT_MAX_WINDOWS,
  DEFAULT_MAX_ROW_ARENA_BYTES,
  DEFAULT_ROW_ARENA_SEGMENT_BYTES,
  UnsupportedAccessModeError,
} from './lib/reader-access.js';
export {
  Direction,
  ExplorerAnchor,
  ExplorerAnchorKind,
  ExplorerComparison,
  ExplorerControl,
  ExplorerError,
  ExplorerFieldMode,
  ExplorerFilter,
  ExplorerFtsPattern,
  ExplorerHistogram,
  ExplorerHistogramBucket,
  ExplorerProgress,
  ExplorerQuery,
  ExplorerResult,
  ExplorerRow,
  ExplorerSampling,
  ExplorerStats,
  ExplorerStopReason,
  ExplorerStrategy,
  ExplorerUnsupported,
  UNSET_VALUE,
  DEFAULT_HISTOGRAM_TARGET_BUCKETS,
  DEFAULT_TIME_SLACK_USEC,
  EXPLORER_CONTROL_CHECK_EVERY_ROWS,
  EXPLORER_PROGRESS_INTERVAL_MS,
};
export {
  FIELD_NAME_POLICY_JOURNALD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW,
} from './lib/writer.js';
export {
  LOG_OPEN_LAZY, LOG_OPEN_EAGER,
  LOG_IDENTITY_AUTO, LOG_IDENTITY_STRICT,
  LOG_LIFECYCLE_CREATED, LOG_LIFECYCLE_ROTATED, LOG_LIFECYCLE_DELETED,
  LOG_LIFECYCLE_REASON_APPEND, LOG_LIFECYCLE_REASON_EAGER_OPEN,
  LOG_LIFECYCLE_REASON_ROTATION, LOG_LIFECYCLE_REASON_RETENTION,
} from './lib/directory-writer.js';
export { FilterBuilder } from './lib/reader.js';
export {
  NetdataJournalFunction,
  NetdataRequest,
  CombinedResult,
  JournalFileCollection,
  normalizeTimeWindow,
  journalFileSourceType,
  collectJournalFiles,
} from './lib/netdata.js';
export {
  SdJournal, SdJournalOpen, SdJournalOpenFile, SdJournalOpenDirectory, SdJournalOpenFiles,
  SdJournalClose,
  SdJournalAddMatch, SdJournalAddDisjunction, SdJournalAddConjunction,
  SdJournalFlushMatches, SdJournalNext, SdJournalNextSkip, SdJournalPrevious,
  SdJournalPreviousSkip,
  SdJournalSeekHead, SdJournalSeekTail, SdJournalSeekRealtimeUsec, SdJournalSeekCursor,
  SdJournalGetEntry, SdJournalGetData, SdJournalRestartData, SdJournalEnumerateAvailableData,
  SdJournalGetRealtimeUsec, SdJournalGetSeqnum, SdJournalGetMonotonicUsec,
  SdJournalGetCursor, SdJournalTestCursor,
  SdJournalEnumerateFields, SdJournalRestartFields, SdJournalEnumerateField,
  SdJournalQueryUnique, SdJournalQueryUniqueState, SdJournalRestartUnique,
  SdJournalEnumerateAvailableUnique, SdJournalVisitUniqueValues, SdJournalListBoots,
  SdJournalSetOutputMode, SdJournalProcessOutput,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from './facade.js';
export { parseMatchString, sipHash24, jenkinsHash64 } from './lib/hash.js';
export { readUint64LE, writeUint64LE, writeUint32LE, writeUint8, align8, bufEqual, uuidToString, stringToUUID, isZeroUUID, randomUUID } from './lib/binary.js';
export { decompressZstSync, isJournalFileName, isZstFile } from './lib/compress.js';
export { verifyFile, verifyFileWithKey, VerificationError } from './lib/verify.js';
export { parseEntryObject, parseDataObject, parseDataPayload } from './lib/entry.js';
export {
  HEADER_SIZE, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
  DATA_OBJECT_HEADER_SIZE, FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE,
  OBJECT_TYPE_DATA, OBJECT_TYPE_FIELD, OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_ENTRY_ARRAY,
  parseFileHeader, parseObjectHeader,
} from './lib/header.js';

// Convenience factory functions
export function openJournal(path, options = {}) {
  return SdJournal.open(path, options);
}

export function createJournal(path, options = {}) {
  return Writer.create(path, options);
}

// String field helper
export function stringField(name, value) {
  return { name, value: Buffer.from(value, 'utf8') };
}

// Binary field helper
export function binaryField(name, value) {
  return { name, value: Buffer.isBuffer(value) ? value : Buffer.from(value) };
}

export default {
  FileReader, DirectoryReader, Writer, Log,
  Direction,
  ExplorerAnchor,
  ExplorerAnchorKind,
  ExplorerComparison,
  ExplorerControl,
  ExplorerError,
  ExplorerFieldMode,
  ExplorerFilter,
  ExplorerFtsPattern,
  ExplorerHistogram,
  ExplorerHistogramBucket,
  ExplorerProgress,
  ExplorerQuery,
  ExplorerResult,
  ExplorerRow,
  ExplorerSampling,
  ExplorerStats,
  ExplorerStopReason,
  ExplorerStrategy,
  ExplorerUnsupported,
  NetdataJournalFunction,
  NetdataRequest,
  CombinedResult,
  JournalFileCollection,
  SdJournal, SdJournalOpen, SdJournalOpenDirectory,
  openJournal, createJournal, stringField, binaryField,
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_LIVE,
  READER_BOUNDS_SNAPSHOT,
  DEFAULT_WINDOW_SIZE_BYTES,
  DEFAULT_MAX_WINDOWS,
  DEFAULT_MAX_ROW_ARENA_BYTES,
  DEFAULT_ROW_ARENA_SEGMENT_BYTES,
  UnsupportedAccessModeError,
  FIELD_NAME_POLICY_JOURNALD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW,
  LOG_OPEN_LAZY, LOG_OPEN_EAGER,
  LOG_IDENTITY_AUTO, LOG_IDENTITY_STRICT,
  LOG_LIFECYCLE_CREATED, LOG_LIFECYCLE_ROTATED, LOG_LIFECYCLE_DELETED,
  LOG_LIFECYCLE_REASON_APPEND, LOG_LIFECYCLE_REASON_EAGER_OPEN,
  LOG_LIFECYCLE_REASON_ROTATION, LOG_LIFECYCLE_REASON_RETENTION,
};
