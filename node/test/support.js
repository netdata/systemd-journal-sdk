import { closeSync, mkdtempSync, rmSync, writeSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { zstdCompressSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { jenkinsHash64, sipHash24 } from '../src/lib/hash.js';
import { uuidToString } from '../src/lib/binary.js';
import {
  DEFAULT_COMPRESS_THRESHOLD, FIELD_NAME_POLICY_JOURNAL_APP, FIELD_NAME_POLICY_RAW,
  MIN_COMPRESS_THRESHOLD, Writer,
} from '../src/lib/writer.js';
import { Log } from '../src/lib/directory-writer.js';
import { FileReader } from '../src/lib/reader.js';
import { DirectoryReader } from '../src/lib/directory-reader.js';
import { parseDataObject, parseEntryObject } from '../src/lib/entry.js';
import {
  exportEntry, jsonEntry, SdJournalOpen, SdJournalOpenFiles, SdJournalQueryUnique,
  SdJournalVisitUniqueValues,
  SdJournalNext, SdJournalPrevious, SdJournalSeekRealtimeUsec,
  SdJournalSeekCursor,
  SdJournalGetEntry, SdJournalGetCursor, SdJournalTestCursor,
  SdJournalGetSeqnum, SdJournalGetMonotonicUsec,
  SdJournalRestartData, SdJournalEnumerateAvailableData, SdJournalGetData,
  SdJournalQueryUniqueState, SdJournalEnumerateAvailableUnique,
  SdJournalRestartFields, SdJournalEnumerateField,
} from '../src/facade.js';
import {
  DATA_OBJECT_HEADER_SIZE,
  ENTRY_OBJECT_HEADER_SIZE,
  COMPACT_DATA_OBJECT_HEADER_SIZE,
  HEADER_SIZE,
  INCOMPATIBLE_COMPACT,
  INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_KEYED_HASH,
  OBJECT_COMPRESSED_LZ4,
  OBJECT_COMPRESSED_XZ,
  OBJECT_COMPRESSED_ZSTD,
  OBJECT_TYPE_DATA,
  OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_TAG,
  FILE_SIZE_INCREASE,
  JOURNAL_COMPACT_SIZE_MAX,
  STATE_ARCHIVED,
  DEFAULT_FIELD_HASH_BUCKETS,
  dataHashBucketsForMaxFileSize,
  parseFileHeader,
  parseObjectHeader,
  writeObjectHeader,
} from '../src/lib/header.js';
import { compressLz4DataPayload } from '../src/lib/lz4-block.js';
import { compressXzDataPayload, decompressXzDataPayload } from '../src/lib/xz-block.js';
import { decompressZstSync } from '../src/lib/compress.js';
import { fsprgGenMK, fsprgGenState0, fsprgEvolve, fsprgSeek, fsprgGetKey, fsprgGetEpoch } from '../src/lib/fss.js';
import { verifyFile, verifyFileWithKey, VerificationError } from '../src/lib/verify.js';
import { SealOptions, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS } from '../src/lib/seal.js';
import { WriterLock } from '../src/lib/lock.js';
import {
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_LIVE,
  READER_BOUNDS_SNAPSHOT,
  UnsupportedAccessModeError,
} from '../src/lib/reader-access.js';
import {
  UNKNOWN_PROCESS_START_TIME,
  lockOwnerIsActive,
  parseLinuxProcStatStartTime,
  readHostBootId,
  readHostBootIdText,
} from '../src/lib/platform.js';
import {
  safeExistsSync,
  safeMkdirSync,
  safeOpenSync,
  safeReadFileSync,
  safeReaddirSync,
  safeStatSync,
  safeSymlinkSync,
  safeWriteFileSync,
} from '../src/lib/fs-safe.js';

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, '..');
const repoRoot = resolve(packageRoot, '..');
const validFSSVerificationKey = 'c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900';

function listJavaScriptFiles(dir, out = []) {
  for (const stat of safeReaddirSync(dir, { withFileTypes: true })) {
    const name = stat.name;
    const path = join(dir, name);
    if (stat.isDirectory()) {
      listJavaScriptFiles(path, out);
    } else if (name.endsWith('.js')) {
      out.push(path);
    }
  }
  return out;
}

function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, {
    cwd: options.cwd || repoRoot,
    encoding: 'utf8',
    input: options.input,
  });
  if (result.status !== 0) {
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    throw new Error(`${cmd} ${args.join(' ')} failed with exit ${result.status}`);
  }
  return result.stdout;
}

function journalFiles(directory) {
  return safeReaddirSync(directory)
    .filter((name) => name.endsWith('.journal'))
    .sort()
    .map((name) => join(directory, name));
}

function disposedJournalFiles(directory) {
  return safeReaddirSync(directory)
    .filter((name) => name.endsWith('.journal~'))
    .sort()
    .map((name) => join(directory, name));
}

function clearKeyedHashFlag(path) {
  const flags = safeReadFileSync(path).readUInt32LE(12);
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(flags & ~INCOMPATIBLE_KEYED_HASH, 0);
  const fd = safeOpenSync(path, 'r+');
  try {
    writeSync(fd, buf, 0, buf.length, 12);
  } finally {
    closeSync(fd);
  }
}

function writeHeaderSize(path, size) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(size), 0);
  const fd = safeOpenSync(path, 'r+');
  try {
    writeSync(fd, buf, 0, buf.length, 88);
  } finally {
    closeSync(fd);
  }
}

function collectNullable(next) {
  const values = [];
  for (;;) {
    const value = next();
    if (value === null || value === undefined) return values;
    values.push(value);
  }
}

function journalctlAvailable() {
  return spawnSync('journalctl', ['--version'], { encoding: 'utf8' }).status === 0;
}

function verifyJournalFileIfAvailable(path) {
  if (journalctlAvailable()) run('journalctl', ['--verify', '--file', path]);
}

function verifyJournalFileFailsIfAvailable(path, expectedText) {
  if (!journalctlAvailable()) return;
  const result = spawnSync('journalctl', ['--verify', '--file', path], { encoding: 'utf8' });
  assert.notEqual(result.status, 0, `journalctl --verify unexpectedly passed for ${path}`);
  const output = `${result.stdout}${result.stderr}`.toLowerCase();
  assert.ok(output.includes(expectedText.toLowerCase()), `journalctl --verify output missing ${expectedText}: ${output}`);
}

function journalctlDirectoryRowsIfAvailable(directory, ...matches) {
  if (!journalctlAvailable()) return null;
  const output = run('journalctl', ['--directory', directory, '--output=json', '--no-pager', ...matches]);
  return output.trim() === '' ? [] : output.trim().split('\n').map((line) => JSON.parse(line));
}

function verifyJournalFileWithKeyIfAvailable(path, key, label = 'journalctl verify') {
  if (!journalctlAvailable()) return;
  const result = spawnSync('journalctl', ['--verify', '--verify-key', key, '--file', path], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`${label} failed: ${result.stderr}`);
  }
  if (!result.stderr.includes('PASS:')) {
    throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
  }
}

function verifyJournalFileWithKeyFailsIfAvailable(path, key) {
  if (!journalctlAvailable()) return;
  const result = spawnSync('journalctl', ['--verify', '--verify-key', key, '--file', path], { encoding: 'utf8' });
  if (result.status === 0) {
    throw new Error(`expected verify to fail, got: ${result.stderr}`);
  }
}

function journalHasDataObjectFlag(path, flag) {
  const buf = safeReadFileSync(path);
  let offset = HEADER_SIZE;

  while (offset + 16 <= buf.length) {
    const header = parseObjectHeader(buf, offset);
    if (!header || header.type === 0 || header.size === 0n) return false;
    if (header.type === OBJECT_TYPE_DATA && (header.flags & flag) !== 0) return true;

    const next = Number(((BigInt(offset) + header.size + 7n) / 8n) * 8n);
    if (next <= offset) return false;
    offset = next;
  }

  return false;
}

function makeHistoricalHeaderFixture(headerSize, incompatibleFlags = INCOMPATIBLE_KEYED_HASH) {
  const buf = Buffer.alloc(Math.max(HEADER_SIZE, headerSize));
  buf.write('LPKSHHRH', 0, 8, 'latin1');
  buf.writeUInt32LE(incompatibleFlags, 12);
  buf.writeBigUInt64LE(BigInt(headerSize), 88);
  buf.writeBigUInt64LE(11n, 208);
  buf.writeBigUInt64LE(22n, 216);
  buf.writeBigUInt64LE(33n, 224);
  buf.writeBigUInt64LE(44n, 232);
  buf.writeBigUInt64LE(55n, 240);
  buf.writeBigUInt64LE(66n, 248);
  buf.writeUInt32LE(77, 256);
  buf.writeUInt32LE(88, 260);
  buf.writeBigUInt64LE(99n, 264);
  return headerSize < HEADER_SIZE ? buf.subarray(0, headerSize) : buf;
}

export {
  closeSync,
  mkdtempSync,
  rmSync,
  writeSync,
  createRequire,
  tmpdir,
  basename,
  dirname,
  join,
  relative,
  resolve,
  fileURLToPath,
  spawnSync,
  zstdCompressSync,
  createHash,
  assert,
  jenkinsHash64,
  sipHash24,
  uuidToString,
  DEFAULT_COMPRESS_THRESHOLD,
  FIELD_NAME_POLICY_JOURNAL_APP,
  FIELD_NAME_POLICY_RAW,
  MIN_COMPRESS_THRESHOLD,
  Writer,
  Log,
  FileReader,
  DirectoryReader,
  parseDataObject,
  parseEntryObject,
  exportEntry,
  jsonEntry,
  SdJournalOpen,
  SdJournalOpenFiles,
  SdJournalQueryUnique,
  SdJournalVisitUniqueValues,
  SdJournalNext,
  SdJournalPrevious,
  SdJournalSeekRealtimeUsec,
  SdJournalSeekCursor,
  SdJournalGetEntry,
  SdJournalGetCursor,
  SdJournalTestCursor,
  SdJournalGetSeqnum,
  SdJournalGetMonotonicUsec,
  SdJournalRestartData,
  SdJournalEnumerateAvailableData,
  SdJournalGetData,
  SdJournalQueryUniqueState,
  SdJournalEnumerateAvailableUnique,
  SdJournalRestartFields,
  SdJournalEnumerateField,
  DATA_OBJECT_HEADER_SIZE,
  ENTRY_OBJECT_HEADER_SIZE,
  COMPACT_DATA_OBJECT_HEADER_SIZE,
  HEADER_SIZE,
  INCOMPATIBLE_COMPACT,
  INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_KEYED_HASH,
  OBJECT_COMPRESSED_LZ4,
  OBJECT_COMPRESSED_XZ,
  OBJECT_COMPRESSED_ZSTD,
  OBJECT_TYPE_DATA,
  OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_TAG,
  FILE_SIZE_INCREASE,
  JOURNAL_COMPACT_SIZE_MAX,
  STATE_ARCHIVED,
  DEFAULT_FIELD_HASH_BUCKETS,
  dataHashBucketsForMaxFileSize,
  parseFileHeader,
  parseObjectHeader,
  writeObjectHeader,
  compressLz4DataPayload,
  compressXzDataPayload,
  decompressXzDataPayload,
  decompressZstSync,
  fsprgGenMK,
  fsprgGenState0,
  fsprgEvolve,
  fsprgSeek,
  fsprgGetKey,
  fsprgGetEpoch,
  verifyFile,
  verifyFileWithKey,
  VerificationError,
  SealOptions,
  COMPATIBLE_SEALED,
  COMPATIBLE_SEALED_CONTINUOUS,
  WriterLock,
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_LIVE,
  READER_BOUNDS_SNAPSHOT,
  UnsupportedAccessModeError,
  UNKNOWN_PROCESS_START_TIME,
  lockOwnerIsActive,
  parseLinuxProcStatStartTime,
  readHostBootId,
  readHostBootIdText,
  safeExistsSync,
  safeMkdirSync,
  safeOpenSync,
  safeReadFileSync,
  safeReaddirSync,
  safeStatSync,
  safeSymlinkSync,
  safeWriteFileSync,
  here,
  packageRoot,
  repoRoot,
  validFSSVerificationKey,
  listJavaScriptFiles,
  run,
  journalFiles,
  disposedJournalFiles,
  clearKeyedHashFlag,
  writeHeaderSize,
  collectNullable,
  journalctlAvailable,
  verifyJournalFileIfAvailable,
  verifyJournalFileFailsIfAvailable,
  journalctlDirectoryRowsIfAvailable,
  verifyJournalFileWithKeyIfAvailable,
  verifyJournalFileWithKeyFailsIfAvailable,
  journalHasDataObjectFlag,
  makeHistoricalHeaderFixture,
};
