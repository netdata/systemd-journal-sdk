// Node.js SDK for systemd journal file reading and writing.
// No native addons, pure JavaScript with Buffer/Uint8Array for binary values.

import { FileReader } from './lib/reader.js';
import { DirectoryReader } from './lib/directory-reader.js';
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
  SdJournalEnumerateAvailableUnique, SdJournalListBoots,
  SdJournalSetOutputMode, SdJournalProcessOutput,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from './facade.js';
import { parseMatchString } from './lib/hash.js';

// Re-export everything
export { FileReader, DirectoryReader, Writer, Log };
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
  SdJournalEnumerateAvailableUnique, SdJournalListBoots,
  SdJournalSetOutputMode, SdJournalProcessOutput,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from './facade.js';
export { parseMatchString, sipHash24, jenkinsHash64 } from './lib/hash.js';
export { readUint64LE, writeUint64LE, writeUint32LE, writeUint8, align8, bufEqual, uuidToString, stringToUUID, isZeroUUID, randomUUID } from './lib/binary.js';
export { decompressZstSync, isJournalFileName, isZstFile } from './lib/compress.js';
export { parseEntryObject, parseDataObject } from './lib/entry.js';
export {
  HEADER_SIZE, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
  DATA_OBJECT_HEADER_SIZE, FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE,
  OBJECT_TYPE_DATA, OBJECT_TYPE_FIELD, OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_ENTRY_ARRAY,
  parseFileHeader, parseObjectHeader,
} from './lib/header.js';

// Convenience factory functions
export function openJournal(path) {
  return SdJournal.open(path);
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
  SdJournal, SdJournalOpen, SdJournalOpenDirectory,
  openJournal, createJournal, stringField, binaryField,
  FIELD_NAME_POLICY_JOURNALD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW,
  LOG_OPEN_LAZY, LOG_OPEN_EAGER,
  LOG_IDENTITY_AUTO, LOG_IDENTITY_STRICT,
  LOG_LIFECYCLE_CREATED, LOG_LIFECYCLE_ROTATED, LOG_LIFECYCLE_DELETED,
  LOG_LIFECYCLE_REASON_APPEND, LOG_LIFECYCLE_REASON_EAGER_OPEN,
  LOG_LIFECYCLE_REASON_ROTATION, LOG_LIFECYCLE_REASON_RETENTION,
};
