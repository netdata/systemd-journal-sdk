#!/usr/bin/env node

import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync, writeSync } from 'node:fs';
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
  UNKNOWN_PROCESS_START_TIME,
  lockOwnerIsActive,
  parseLinuxProcStatStartTime,
  readHostBootId,
  readHostBootIdText,
} from '../src/lib/platform.js';

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, '..');
const repoRoot = resolve(packageRoot, '..');
const validFSSVerificationKey = 'c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900';

function listJavaScriptFiles(dir, out = []) {
  for (const stat of readdirSync(dir, { withFileTypes: true })) {
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
  return readdirSync(directory)
    .filter((name) => name.endsWith('.journal'))
    .sort()
    .map((name) => join(directory, name));
}

function disposedJournalFiles(directory) {
  return readdirSync(directory)
    .filter((name) => name.endsWith('.journal~'))
    .sort()
    .map((name) => join(directory, name));
}

function clearKeyedHashFlag(path) {
  const flags = readFileSync(path).readUInt32LE(12);
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(flags & ~INCOMPATIBLE_KEYED_HASH, 0);
  const fd = openSync(path, 'r+');
  try {
    writeSync(fd, buf, 0, buf.length, 12);
  } finally {
    closeSync(fd);
  }
}

function writeHeaderSize(path, size) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(size), 0);
  const fd = openSync(path, 'r+');
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
  const buf = readFileSync(path);
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

const historicalHeaderCases = [
  { headerSize: 208 },
  { headerSize: 216, n_data: 11n },
  { headerSize: 220, n_data: 11n },
  { headerSize: 224, n_data: 11n, n_fields: 22n },
  { headerSize: 232, n_data: 11n, n_fields: 22n, n_tags: 33n },
  { headerSize: 240, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n },
  { headerSize: 248, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n },
  { headerSize: 250, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n },
  { headerSize: 256, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n },
  { headerSize: 260, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77 },
  { headerSize: 264, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88 },
  { headerSize: 268, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88 },
  { headerSize: 272, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88, tail_entry_offset: 99n },
  { headerSize: 300, n_data: 11n, n_fields: 22n, n_tags: 33n, n_entry_arrays: 44n, data_hash_chain_depth: 55n, field_hash_chain_depth: 66n, tail_entry_array_offset: 77, tail_entry_array_n_entries: 88, tail_entry_offset: 99n },
];

for (const expected of historicalHeaderCases) {
  const header = parseFileHeader(makeHistoricalHeaderFixture(expected.headerSize));
  assert.equal(header.n_data, expected.n_data ?? 0n, `n_data header_size=${expected.headerSize}`);
  assert.equal(header.n_fields, expected.n_fields ?? 0n, `n_fields header_size=${expected.headerSize}`);
  assert.equal(header.n_tags, expected.n_tags ?? 0n, `n_tags header_size=${expected.headerSize}`);
  assert.equal(header.n_entry_arrays, expected.n_entry_arrays ?? 0n, `n_entry_arrays header_size=${expected.headerSize}`);
  assert.equal(header.data_hash_chain_depth, expected.data_hash_chain_depth ?? 0n, `data_hash_chain_depth header_size=${expected.headerSize}`);
  assert.equal(header.field_hash_chain_depth, expected.field_hash_chain_depth ?? 0n, `field_hash_chain_depth header_size=${expected.headerSize}`);
  assert.equal(header.tail_entry_array_offset, expected.tail_entry_array_offset ?? 0, `tail_entry_array_offset header_size=${expected.headerSize}`);
  assert.equal(header.tail_entry_array_n_entries, expected.tail_entry_array_n_entries ?? 0, `tail_entry_array_n_entries header_size=${expected.headerSize}`);
  assert.equal(header.tail_entry_offset, expected.tail_entry_offset ?? 0n, `tail_entry_offset header_size=${expected.headerSize}`);
}

assert.throws(
  () => parseFileHeader(makeHistoricalHeaderFixture(300).subarray(0, 208)),
  /header buffer too small/,
  'future header with truncated known prefix should be rejected',
);

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-historical-unkeyed-'));
  const journalPath = join(tempDir, 'unkeyed-lz4.journal');
  try {
    writeFileSync(journalPath, makeHistoricalHeaderFixture(240, INCOMPATIBLE_COMPRESSED_LZ4));
    const reader = FileReader.open(journalPath);
    assert.equal(reader.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH, 0);
    assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4);
    reader.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

const jenkinsVectors = [
  ['', 0xdeadbeefdeadbeefn],
  ['SYSLOG_IDENTIFIER=netdata', 0x45ccd0e9ed13614an],
  ['_SYSTEMD_UNIT=netdata.service', 0x1013c5df11a983f0n],
  ['PRIORITY=6', 0x80f09f19808d26a3n],
  ['MESSAGE=Test message', 0x8ed53fb52aa5c55dn],
];

for (const [data, expected] of jenkinsVectors) {
  assert.equal(jenkinsHash64(Buffer.from(data)), expected, `jenkinsHash64(${JSON.stringify(data)})`);
}

const sipKey = Buffer.from(Array.from({ length: 16 }, (_, i) => i));
const sipMessage = Buffer.from(Array.from({ length: 64 }, (_, i) => i));
const sipVectors = [
  [0, 0x726fdb47dd0e0e31n],
  [1, 0x74f839c593dc67fdn],
  [2, 0x0d6c8009d9a94f5an],
  [3, 0x85676696d7fb7e2dn],
  [4, 0xcf2794e0277187b7n],
  [5, 0x18765564cd99a68dn],
  [6, 0xcbc9466e58fee3cen],
  [7, 0xab0200f58b01d137n],
];

for (const [length, expected] of sipVectors) {
  assert.equal(sipHash24(sipKey, sipMessage.subarray(0, length)), expected, `sipHash24(length=${length})`);
}

{
  const key = Buffer.from('de5f2812d87b89e81af97cfe8e1423e9', 'hex');
  const payload = Buffer.concat([
    Buffer.from('COMPRESSED_PAYLOAD='),
    Buffer.from(Array.from({ length: 256 }, (_, i) => (i % 26) + 0x41)),
  ]);
  assert.equal(sipHash24(key, payload), 0xf9a795df589b5204n);
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-live-publish-'));
  try {
    const writeFile = (name, every) => {
      const journalPath = join(tempDir, `${name}.journal`);
      const writer = Writer.create(journalPath, {
        fileId: Buffer.from('40000000000000000000000000000000', 'hex'),
        machineId: Buffer.from('10000000000000000000000000000000', 'hex'),
        bootId: Buffer.from('20000000000000000000000000000000', 'hex'),
        seqnumId: Buffer.from('30000000000000000000000000000000', 'hex'),
        dataHashTableBuckets: 64,
        fieldHashTableBuckets: 16,
        livePublishEveryEntries: every,
      });
      for (let i = 0; i < 5; i++) {
        writer.append([
          { name: 'MESSAGE', value: `row-${String(i).padStart(2, '0')}` },
          { name: 'SYSLOG_IDENTIFIER', value: 'node-live-publish-test' },
        ], { realtimeUsec: 1_700_000_100_000_000n + BigInt(i), monotonicUsec: BigInt(i + 1) });
      }
      const pending = writer.entriesSinceLivePublication;
      writer.close();
      return { bytes: readFileSync(journalPath), pending };
    };

    const immediate = writeFile('immediate', 1);
    const disabled = writeFile('disabled', 0);
    const everyThree = writeFile('every-three', 3);
    assert.equal(immediate.pending, 0);
    assert.equal(disabled.pending, 0);
    assert.equal(everyThree.pending, 2);
    assert.deepEqual(disabled.bytes, immediate.bytes);
    assert.deepEqual(everyThree.bytes, immediate.bytes);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const statFields = Array.from({ length: 20 }, (_, i) => String(i + 1));
  statFields[19] = '424242';
  assert.equal(parseLinuxProcStatStartTime(`123 (node worker) ${statFields.join(' ')}`), '424242');

  assert.equal(lockOwnerIsActive(
    { pid: 123, bootId: 'boot-a', startTime: '1' },
    { bootId: 'boot-b', processStartTime: () => '1', processAlive: () => true },
  ), false);
  assert.equal(lockOwnerIsActive(
    { pid: 123, bootId: '', startTime: '1' },
    { bootId: '', processStartTime: () => '2', processAlive: () => true },
  ), false);
  assert.equal(lockOwnerIsActive(
    { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
    { bootId: '', processStartTime: () => null, processAlive: () => false },
  ), false);
  assert.equal(lockOwnerIsActive(
    { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
    { bootId: '', processStartTime: () => null, processAlive: () => true },
  ), true);
  assert.equal(lockOwnerIsActive(
    { pid: 123, bootId: '', startTime: UNKNOWN_PROCESS_START_TIME },
    { bootId: '', processStartTime: () => null, processAlive: () => null },
  ), true);

  const hostBootId = readHostBootId();
  if (hostBootId !== null) assert.equal(hostBootId.length, 16);
  const hostBootIdText = readHostBootIdText();
  if (hostBootIdText) {
    assert.match(hostBootIdText, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'jf-facade.journal');
    const writer = Writer.create(journalPath);
    writer.append([
      { name: 'MESSAGE', value: 'first' },
      { name: 'REPEAT', value: 'one' },
      { name: 'REPEAT', value: 'two' },
      { name: 'BIN', value: Buffer.from([0x00, 0xff]) },
    ], { realtimeUsec: 1000n, monotonicUsec: 11n });
    writer.append([
      { name: 'MESSAGE', value: 'second' },
      { name: 'REPEAT', value: 'three' },
    ], { realtimeUsec: 1001n, monotonicUsec: 12n });
    writer.close();

    const journal = SdJournalOpenFiles([journalPath], 0);
    assert.equal(SdJournalNext(journal), 1);
    const seqnum = SdJournalGetSeqnum(journal);
    assert.equal(seqnum.seqnum, 1n);
    assert.ok(seqnum.seqnum_id);
    const monotonic = SdJournalGetMonotonicUsec(journal);
    assert.equal(monotonic.monotonic, 11n);
    assert.ok(Buffer.isBuffer(monotonic.boot_id));

    SdJournalRestartData(journal);
    const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
    assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=one'))));
    assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=two'))));
    assert.ok(payloads.some(p => p.equals(Buffer.from([0x42, 0x49, 0x4e, 0x3d, 0x00, 0xff]))));
    assert.deepEqual(SdJournalGetData(journal, 'REPEAT'), Buffer.from('REPEAT=one'));

    SdJournalQueryUniqueState(journal, 'REPEAT');
    const unique = collectNullable(() => SdJournalEnumerateAvailableUnique(journal));
    assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=one'))));
    assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=two'))));
    assert.ok(unique.some(p => p.equals(Buffer.from('REPEAT=three'))));

    SdJournalRestartFields(journal);
    const fields = new Set(collectNullable(() => SdJournalEnumerateField(journal)));
    assert.ok(fields.has('MESSAGE'));
    assert.ok(fields.has('REPEAT'));
    assert.ok(fields.has('BIN'));

    SdJournalSeekRealtimeUsec(journal, 1001n);
    assert.equal(SdJournalNext(journal), 1);
    assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
    SdJournalSeekRealtimeUsec(journal, 1001n);
    assert.equal(SdJournalPrevious(journal), 1);
    assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
    const cursor = SdJournalGetCursor(journal);
    assert.equal(SdJournalTestCursor(journal, cursor), true);
    assert.equal(SdJournalTestCursor(journal, 'invalid-cursor'), false);
    SdJournalSeekRealtimeUsec(journal, 1000n);
    assert.equal(SdJournalNext(journal), 1);
    assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'first');
    SdJournalSeekCursor(journal, cursor);
    assert.equal(SdJournalGetEntry(journal).fields.MESSAGE.toString('utf8'), 'second');
    const missingCursor = cursor.replace(/n=\d+$/, 'n=999999');
    assert.doesNotThrow(() => SdJournalSeekCursor(journal, missingCursor));
    journal.close();

    const journalPath2 = join(tempDir, 'jf-facade-second.journal');
    const writer2 = Writer.create(journalPath2);
    writer2.append([
      { name: 'MESSAGE', value: 'third' },
      { name: 'REPEAT', value: 'four' },
    ], { realtimeUsec: 1002n, monotonicUsec: 21n });
    writer2.close();

    const multi = SdJournalOpenFiles([journalPath2, journalPath], 0);
    const messages = [];
    while (SdJournalNext(multi) === 1) {
      messages.push(SdJournalGetEntry(multi).fields.MESSAGE.toString('utf8'));
    }
    assert.deepEqual(messages, ['first', 'second', 'third']);
    SdJournalSeekRealtimeUsec(multi, 1002n);
    assert.equal(SdJournalPrevious(multi), 1);
    assert.equal(SdJournalGetEntry(multi).fields.MESSAGE.toString('utf8'), 'third');
    SdJournalSeekRealtimeUsec(multi, 999n);
    assert.equal(SdJournalPrevious(multi), 0);
    multi.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'facade-row-lifetime.journal');
    const writer = Writer.create(journalPath);
    writer.append([
      { name: 'MESSAGE', value: 'first' },
      { name: 'REPEAT', value: 'one' },
      { name: 'REPEAT', value: 'two' },
    ], { realtimeUsec: 1000n, monotonicUsec: 11n });
    writer.close();

    const journal = SdJournalOpenFiles([journalPath], 0);
    assert.equal(SdJournalNext(journal), 1);
    SdJournalRestartData(journal);
    const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
    assert.ok(payloads.some(p => p.equals(Buffer.from('MESSAGE=first'))));
    assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=one'))));
    assert.ok(payloads.some(p => p.equals(Buffer.from('REPEAT=two'))));
    journal.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'facade-compressed-row-lifetime.journal');
    const writer = Writer.create(journalPath, { compression: 'zstd', compressionThresholdBytes: 8 });
    const largeValue = Buffer.from('mixed '.repeat(256));
    writer.append([
      { name: 'SMALL', value: 'x' },
      { name: 'LARGE', value: largeValue },
    ], { realtimeUsec: 1000n, monotonicUsec: 11n });
    writer.close();

    const journal = SdJournalOpenFiles([journalPath], 0);
    assert.equal(SdJournalNext(journal), 1);
    SdJournalRestartData(journal);
    const payloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
    assert.ok(payloads.some(p => p.equals(Buffer.from('SMALL=x'))));
    assert.ok(payloads.some(p => p.equals(Buffer.concat([Buffer.from('LARGE='), largeValue]))));
    journal.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'raw-byte-names.journal');
    const invalidUtf8Name = Buffer.from([0xff, 0x52, 0x41, 0x57]);
    const nulName = Buffer.from('RAW\0NAME', 'latin1');
    const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_RAW });
    writer.append([
      { name: 'MESSAGE', value: 'raw byte names' },
      { name: invalidUtf8Name, value: Buffer.from('invalid utf8') },
      { name: nulName, value: Buffer.from('nul name') },
      { name: 'field name', value: Buffer.from('space') },
      { name: 'BINARY', value: Buffer.from('a\0=b', 'latin1') },
    ], { realtimeUsec: 1_700_004_000_000_000n, monotonicUsec: 1n });
    writer.close();

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw byte names');
    assert.equal(entry.rawFieldValues.get(invalidUtf8Name.toString('hex'))[0].toString('utf8'), 'invalid utf8');
    assert.equal(entry.rawFieldValues.get(nulName.toString('hex'))[0].toString('utf8'), 'nul name');
    assert.equal(entry.rawFieldValues.get(Buffer.from('field name').toString('hex'))[0].toString('utf8'), 'space');
    assert.equal(reader.getRaw(invalidUtf8Name).toString('utf8'), 'invalid utf8');
    assert.equal(reader.getRaw(nulName).toString('utf8'), 'nul name');
    assert.deepEqual(reader.getRawValues(Buffer.from('field name')).map(v => v.toString('utf8')), ['space']);
    assert.ok(entry.rawFields.some(([name, value]) => name.equals(invalidUtf8Name) && value.equals(Buffer.from('invalid utf8'))));
    assert.ok(entry.payloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
    assert.equal(Object.prototype.hasOwnProperty.call(entry.fields, invalidUtf8Name.toString('utf8')), false);

    const payloads = [];
    reader.visitEntryPayloads((payload) => payloads.push(Buffer.from(payload)));
    assert.ok(payloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
    reader.close();

    const exported = exportEntry(entry);
    assert.ok(exported.includes(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8\n')])));
    const encoded = jsonEntry(entry);
    assert.equal(Object.prototype.hasOwnProperty.call(encoded, invalidUtf8Name.toString('utf8')), false);

    const dirReader = DirectoryReader.openFiles([journalPath]);
    assert.equal(dirReader.step(), true);
    assert.equal(dirReader.getRaw(invalidUtf8Name).toString('utf8'), 'invalid utf8');
    dirReader.close();

    const journal = SdJournalOpen(journalPath, 0);
    assert.equal(SdJournalNext(journal), 1);
    assert.deepEqual(SdJournalGetData(journal, Buffer.from('BINARY')), Buffer.from('BINARY=a\0=b', 'latin1'));
    const originalGetEntryPayload = journal.reader.getEntryPayload;
    journal.reader.getEntryPayload = undefined;
    assert.deepEqual(SdJournalGetData(journal, invalidUtf8Name), Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]));
    journal.reader.getEntryPayload = originalGetEntryPayload;
    SdJournalRestartData(journal);
    const facadePayloads = collectNullable(() => SdJournalEnumerateAvailableData(journal));
    journal.close();
    assert.ok(facadePayloads.some(p => p.equals(Buffer.concat([invalidUtf8Name, Buffer.from('=invalid utf8')]))));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'active-reader-refresh.journal');
    const writer = Writer.create(journalPath, { livePublishEveryEntries: 1 });
    writer.append([{ name: 'MESSAGE', value: 'first' }], { realtimeUsec: 1_700_004_010_000_000n, monotonicUsec: 1n });
    const reader = FileReader.open(journalPath);
    reader.seekHead();
    assert.equal(reader.step(), true);
    assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'first');
    assert.equal(reader.step(), false);
    writer.append([{ name: 'MESSAGE', value: 'second' }], { realtimeUsec: 1_700_004_010_000_001n, monotonicUsec: 2n });
    assert.equal(reader.step(), true);
    assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'second');
    reader.close();
    writer.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const dataBuf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 4);
  writeObjectHeader(dataBuf, 0, OBJECT_TYPE_DATA, 0, BigInt(DATA_OBJECT_HEADER_SIZE + 16));
  assert.throws(() => parseDataObject(dataBuf, 0), /data object exceeds buffer/);

  const entryBuf = Buffer.alloc(ENTRY_OBJECT_HEADER_SIZE);
  writeObjectHeader(entryBuf, 0, OBJECT_TYPE_ENTRY, 0, BigInt(ENTRY_OBJECT_HEADER_SIZE + 16));
  assert.throws(() => parseEntryObject(entryBuf, 0), /entry object exceeds buffer/);
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      retentionPolicy: { maxAgeUsec: 20_000_001n },
    });
    const base = BigInt(Date.now()) * 1000n;
    for (const [i, realtime] of [base, base + 1_000_000n, base + 1_000_001n].entries()) {
      log.append([
        { name: 'MESSAGE', value: `derived-duration-rotation-${i}` },
        { name: 'TEST_ID', value: 'derived-duration-rotation' },
      ], {
        realtimeUsec: realtime,
        monotonicUsec: BigInt(i + 1),
      });
    }
    log.close();
    const files = journalFiles(log.journalDirectory());
    assert.equal(files.length, 2);
    const counts = files.map((path) => {
      const reader = FileReader.open(path);
      try {
        return reader.header.n_entries;
      } finally {
        reader.close();
      }
    });
    assert.deepEqual(counts, [2n, 1n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      retentionPolicy: { maxBytes: 1_000_000 },
    });
    assert.equal(log.maxBytes, 512 * 1024);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const maxSize = 128 * 1024 * 1024;
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      retentionPolicy: { maxBytes: maxSize * 20, maxAgeUsec: 20_000_001n },
    });
    assert.equal(log.maxBytes, maxSize);
    assert.equal(log.maxDurationUsec, 1_000_001n);
    log.append([{ name: 'MESSAGE', value: 'derived rotation defaults' }], {
      realtimeUsec: 1_700_002_091_000_000n,
      monotonicUsec: 1n,
    });
    log.close();
    const files = journalFiles(log.journalDirectory());
    assert.equal(files.length, 1);
    const reader = FileReader.open(files[0]);
    try {
      assert.equal(Number(reader.header.data_hash_table_size / 16n), dataHashBucketsForMaxFileSize(maxSize));
      assert.equal(Number(reader.header.field_hash_table_size / 16n), DEFAULT_FIELD_HASH_BUCKETS);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const maxSize = 16 * 1024 * 1024;
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      retentionPolicy: { maxBytes: maxSize * 20 },
    });
    assert.equal(log.maxBytes, maxSize);
    for (let i = 0; i < 12; i++) {
      log.append([
        { name: 'MESSAGE', value: `derived-size-rotation-${i}` },
        { name: 'PAYLOAD', value: `${String(i).padStart(5, '0')}-${'x'.repeat(2 * 1024 * 1024)}` },
        { name: 'TEST_ID', value: 'derived-size-rotation' },
      ], {
        realtimeUsec: 1_700_002_092_000_000n + BigInt(i),
        monotonicUsec: BigInt(i + 1),
      });
    }
    log.close();
    const files = journalFiles(log.journalDirectory());
    assert.ok(files.length >= 2);
    let entries = 0n;
    for (const path of files) {
      const reader = FileReader.open(path);
      try {
        assert.equal(Number(reader.header.data_hash_table_size / 16n), dataHashBucketsForMaxFileSize(maxSize));
        entries += reader.header.n_entries;
      } finally {
        reader.close();
      }
    }
    assert.equal(entries, 12n);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      compact: true,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      retentionPolicy: { maxBytes: (Number(JOURNAL_COMPACT_SIZE_MAX) + 4096) * 20 },
    });
    assert.equal(log.maxBytes, Number(JOURNAL_COMPACT_SIZE_MAX));
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const explicitSize = 64 * 1024 * 1024;
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      rotationPolicy: { maxBytes: explicitSize, maxDurationUsec: 2_000_000n },
      retentionPolicy: { maxBytes: 128 * 1024 * 1024 * 20, maxAgeUsec: 20_000_000n },
    });
    assert.equal(log.maxBytes, explicitSize);
    assert.equal(log.maxDurationUsec, 2_000_000n);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const machineId = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
    const bootA = Buffer.from('aa000000000000000000000000000001', 'hex');
    const bootB = Buffer.from('bb000000000000000000000000000002', 'hex');
    const first = new Log(tempDir, {
      source: 'system',
      identityMode: 'strict',
      machineId,
      bootId: bootA,
    });
    first.append(
      [{ name: 'MESSAGE', value: 'cross boot first' }, { name: 'TEST_ID', value: 'cross-boot-monotonic' }],
      { realtimeUsec: 1_700_003_100_000_000n, monotonicUsec: 100n },
    );
    first.close();

    const second = new Log(tempDir, {
      source: 'system',
      identityMode: 'strict',
      machineId,
      bootId: bootB,
    });
    second.append(
      [{ name: 'MESSAGE', value: 'cross boot second' }, { name: 'TEST_ID', value: 'cross-boot-monotonic' }],
      { realtimeUsec: 1_700_003_100_000_001n, monotonicUsec: 1n },
    );
    second.close();

    const entries = [];
    for (const path of journalFiles(join(tempDir, uuidToString(machineId)))) {
      verifyJournalFileIfAvailable(path);
      const reader = FileReader.open(path);
      while (reader.step()) {
        const entry = reader.getEntry();
        if (entry.fields.TEST_ID?.toString('utf8') === 'cross-boot-monotonic') entries.push(entry);
      }
      reader.close();
    }
    entries.sort((a, b) => (a.realtime < b.realtime ? -1 : a.realtime > b.realtime ? 1 : 0));
    assert.equal(entries.length, 2);
    assert.deepEqual(entries.map((entry) => entry.monotonic), [100n, 1n]);
    assert.deepEqual(
      entries.map((entry) => uuidToString(entry.boot_id)),
      [uuidToString(bootA), uuidToString(bootB)],
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'raw-zero-monotonic.journal');
    const writer = Writer.create(journalPath);
    writer.append([{ name: 'MESSAGE', value: 'raw zero monotonic' }], {
      realtimeUsec: 1_700_003_000_100_000n,
      monotonicUsec: 0n,
    });
    writer.close();
    verifyJournalFileIfAvailable(journalPath);

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.monotonic, 0n);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  let writer;
  try {
    const journalPath = join(tempDir, 'journald-field-policy.journal');
    writer = Writer.create(journalPath);
    writer.append([
      { name: 'MESSAGE', value: 'trusted fields' },
      { name: '_HOSTNAME', value: 'synthetic-host' },
      { name: '_TRANSPORT', value: 'journal' },
    ], { realtimeUsec: 1_700_002_111_000_000n, monotonicUsec: 1n });
    for (const invalidName of ['lowercase', 'foo.bar', 'A'.repeat(65), '1FIELD']) {
      assert.throws(
        () => writer.append([{ name: invalidName, value: 'invalid' }], {
          realtimeUsec: 1_700_002_111_000_001n,
          monotonicUsec: 2n,
        }),
        /invalid field name/,
      );
    }
    writer.close();
    writer = undefined;
    verifyJournalFileIfAvailable(journalPath);

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
    assert.equal(entry.fields._TRANSPORT.toString('utf8'), 'journal');
  } finally {
    try {
      if (writer) writer.close();
    } catch {
      // Ignore cleanup errors from already closed writers in failing tests.
    }
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'raw-journald-payloads.journal');
    const writer = Writer.create(journalPath);
    writer.appendRaw([
      Buffer.from('MESSAGE=raw full payload'),
      Buffer.from('_HOSTNAME=synthetic-host'),
      Buffer.from('BINARY=a\x00=b=c', 'utf8'),
    ], { realtimeUsec: 1_700_002_111_100_000n, monotonicUsec: 2n });
    assert.throws(
      () => writer.appendRaw([], {
        realtimeUsec: 1_700_002_111_100_001n,
        monotonicUsec: 3n,
      }),
      /empty entry/,
    );
    assert.throws(
      () => writer.appendRaw([Buffer.from('=value')], {
        realtimeUsec: 1_700_002_111_100_002n,
        monotonicUsec: 4n,
      }),
      /invalid raw field payload/,
    );
    writer.close();
    verifyJournalFileIfAvailable(journalPath);

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw full payload');
    assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
    assert.deepEqual(entry.fields.BINARY, Buffer.from('a\x00=b=c', 'utf8'));
    assert.equal(entry.fields._BOOT_ID, undefined);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const write = (name, raw) => {
      const journalPath = join(tempDir, `${name}.journal`);
      const writer = Writer.create(journalPath, {
        fileId: Buffer.from('40000000000000000000000000000000', 'hex'),
        machineId: Buffer.from('10000000000000000000000000000000', 'hex'),
        bootId: Buffer.from('20000000000000000000000000000000', 'hex'),
        seqnumId: Buffer.from('30000000000000000000000000000000', 'hex'),
        dataHashTableBuckets: 64,
        fieldHashTableBuckets: 16,
      });
      const opts = { realtimeUsec: 1_700_002_111_200_000n, monotonicUsec: 3n };
      if (raw) {
        writer.appendRaw([
          Buffer.from('MESSAGE=equivalent entry'),
          Buffer.from('PRIORITY=6'),
          Buffer.from('BINARY=a\x00=b=c', 'utf8'),
        ], opts);
      } else {
        writer.append([
          { name: 'MESSAGE', value: 'equivalent entry' },
          { name: 'PRIORITY', value: '6' },
          { name: 'BINARY', value: Buffer.from('a\x00=b=c', 'utf8') },
        ], opts);
      }
      writer.close();
      return readFileSync(journalPath);
    };

    assert.deepEqual(write('structured', false), write('raw', true));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const bootId = '0123456789abcdef0123456789abcdef';
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      bootId: Buffer.from(bootId, 'hex'),
    });
    log.appendRaw([
      Buffer.from('MESSAGE=raw journald directory'),
      Buffer.from('TEST_ID=node-log-append-raw-journald'),
      Buffer.from('_HOSTNAME=synthetic-host'),
      Buffer.from(`_BOOT_ID=${bootId}`),
    ], {
      realtimeUsec: 1_700_002_403_500_000n,
      monotonicUsec: 35n,
      sourceRealtimeUsec: 999n,
    });
    const journalDir = log.journalDirectory();
    log.close();

    const path = journalFiles(journalDir)[0];
    const reader = FileReader.open(path);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw journald directory');
    assert.equal(entry.fields.TEST_ID.toString('utf8'), 'node-log-append-raw-journald');
    assert.equal(entry.fields._HOSTNAME.toString('utf8'), 'synthetic-host');
    assert.equal(entry.fields._BOOT_ID.toString('utf8'), bootId);
    assert.equal(entry.fieldValues._BOOT_ID.length, 1);
    assert.equal(entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8'), '999');
    verifyJournalFileIfAvailable(path);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    assert.throws(
      () => Writer.create(join(tempDir, 'alias-systemd.journal'), { fieldNamePolicy: 'systemd' }),
      /unsupported field name policy/,
    );
    assert.throws(
      () => Writer.create(join(tempDir, 'alias-app.journal'), { fieldNamePolicy: 'app' }),
      /unsupported field name policy/,
    );
    assert.throws(
      () => Writer.create(join(tempDir, 'alias-journal-app.journal'), { fieldNamePolicy: 'journal_app' }),
      /unsupported field name policy/,
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'journal-app-field-policy.journal');
    const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP });
    writer.append([
      { name: 'MESSAGE', value: 'app valid' },
      { name: '_HOSTNAME', value: 'drop-host' },
      { name: 'lowercase', value: 'drop-lowercase' },
    ], { realtimeUsec: 1_700_002_112_000_000n, monotonicUsec: 1n });
    assert.throws(
      () => writer.append([{ name: '_HOSTNAME', value: 'drop-only' }], {
        realtimeUsec: 1_700_002_112_000_001n,
        monotonicUsec: 2n,
      }),
      /empty entry/,
    );
    writer.close();
    verifyJournalFileIfAvailable(journalPath);

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'app valid');
    assert.equal(entry.fields._HOSTNAME, undefined);
    assert.equal(entry.fields.lowercase, undefined);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'raw-journal-app-field-policy.journal');
    const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP });
    writer.appendRaw([
      Buffer.from('MESSAGE=raw app valid'),
      Buffer.from('_HOSTNAME=drop-host'),
      Buffer.from('lowercase=drop-lowercase'),
    ], { realtimeUsec: 1_700_002_112_100_000n, monotonicUsec: 3n });
    assert.throws(
      () => writer.appendRaw([Buffer.from('_HOSTNAME=drop-only')], {
        realtimeUsec: 1_700_002_112_100_001n,
        monotonicUsec: 4n,
      }),
      /empty entry/,
    );
    assert.throws(
      () => writer.appendRaw([Buffer.from('NO_EQUALS')], {
        realtimeUsec: 1_700_002_112_100_002n,
        monotonicUsec: 5n,
      }),
      /invalid raw field payload/,
    );
    writer.close();
    verifyJournalFileIfAvailable(journalPath);

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw app valid');
    assert.equal(entry.fields._HOSTNAME, undefined);
    assert.equal(entry.fields.lowercase, undefined);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const longName = 'a'.repeat(1024);
    const journalPath = join(tempDir, 'raw-field-policy.journal');
    const writer = Writer.create(journalPath, { fieldNamePolicy: FIELD_NAME_POLICY_RAW });
    writer.append([
      { name: 'lowercase', value: 'ok' },
      { name: 'foo.bar', value: 'dot' },
      { name: 'field name', value: 'space' },
      { name: longName, value: 'long' },
      { name: 'BINARY', value: Buffer.from([0x61, 0x00, 0x3d, 0x62]) },
    ], { realtimeUsec: 1_700_002_113_000_000n, monotonicUsec: 1n });
    assert.throws(
      () => writer.append([{ name: 'BAD=NAME', value: 'bad' }], {
        realtimeUsec: 1_700_002_113_000_001n,
        monotonicUsec: 2n,
      }),
      /invalid field name/,
    );
    writer.close();

    const reader = FileReader.open(journalPath);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.lowercase.toString('utf8'), 'ok');
    assert.equal(entry.fields['foo.bar'].toString('utf8'), 'dot');
    assert.equal(entry.fields['field name'].toString('utf8'), 'space');
    assert.equal(entry.fields[longName].toString('utf8'), 'long');
    assert.deepEqual(entry.fields.BINARY, Buffer.from([0x61, 0x00, 0x3d, 0x62]));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'raw-backward-monotonic.journal');
    const writer = Writer.create(journalPath);
    writer.append([{ name: 'MESSAGE', value: 'raw monotonic first' }], {
      realtimeUsec: 1_700_003_000_000_000n,
      monotonicUsec: 10n,
    });
    writer.append([{ name: 'MESSAGE', value: 'raw monotonic second' }], {
      realtimeUsec: 1_700_003_000_000_001n,
      monotonicUsec: 5n,
    });
    writer.close();

    assert.throws(() => verifyFile(journalPath), /monotonic/);
    verifyJournalFileFailsIfAvailable(journalPath, 'timestamp out of synchronization');
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'unsupported-flags.journal');
    const writer = Writer.create(journalPath);
    writer.close();

    const fd = openSync(journalPath, 'r+');
    const flags = Buffer.alloc(4);
    const unsupportedFlag = 1 << 30;
    flags.writeUInt32LE(INCOMPATIBLE_KEYED_HASH | unsupportedFlag, 0);
    writeSync(fd, flags, 0, flags.length, 12);
    closeSync(fd);

    assert.throws(() => Writer.open(journalPath), /unsupported journal: incompatible flags/);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  // Node compact writer -> Node reader and stock journalctl round-trip.
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'compact-writer.journal');
    const writer = Writer.create(journalPath, { compact: true });
    for (let i = 0; i < 3; i++) {
      writer.append([
        { name: 'MESSAGE', value: `compact-${i}` },
        { name: 'TEST_ID', value: 'node-compact' },
        { name: 'REUSED', value: 'same' },
      ], {
        realtimeUsec: 1_700_000_040_000_000n + BigInt(i),
        monotonicUsec: BigInt(i + 1),
      });
    }
    writer.close();

    const reader = FileReader.open(journalPath);
    assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPACT, 'compact flag must be set');
    const messages = [];
    while (reader.next()) messages.push(reader.getEntry().fields.MESSAGE.toString('utf8'));
    reader.close();
    assert.deepEqual(messages, ['compact-0', 'compact-1', 'compact-2']);

    const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
    if (journalctl.status === 0) {
      run('journalctl', ['--verify', '--file', journalPath]);
      const output = run('journalctl', ['--file', journalPath, '--output=json', '--no-pager', 'TEST_ID=node-compact']);
      assert.equal(output.trim().split('\n').filter(Boolean).length, 3);
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'compact-grown.journal');
    const writer = Writer.create(journalPath, { compact: true });
    for (let i = 0; i < 10; i++) {
      writer.append([
        { name: 'BLOB', value: Buffer.alloc(1024 * 1024, i) },
      ], {
        realtimeUsec: 1_700_000_050_000_000n + BigInt(i),
        monotonicUsec: BigInt(i + 1),
      });
    }
    writer.close();

    const header = parseFileHeader(readFileSync(journalPath).subarray(0, HEADER_SIZE));
    assert.ok(
      header.arena_size + BigInt(HEADER_SIZE) > BigInt(FILE_SIZE_INCREASE),
      'arena size must grow past initial allocation',
    );
    const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
    if (journalctl.status === 0) run('journalctl', ['--verify', '--file', journalPath]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'large-hash-table.journal');
    const writer = Writer.create(journalPath, {
      compact: true,
      dataHashTableBuckets: 600000,
      fieldHashTableBuckets: 1023,
    });
    writer.append([
      { name: 'MESSAGE', value: Buffer.from('large hash table') },
    ], {
      realtimeUsec: 1_700_000_060_000_000n,
      monotonicUsec: 1n,
    });
    writer.close();

    const header = parseFileHeader(readFileSync(journalPath).subarray(0, HEADER_SIZE));
    assert.ok(
      header.arena_size + BigInt(HEADER_SIZE) > BigInt(FILE_SIZE_INCREASE),
      'initial arena must cover large hash tables',
    );
    const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
    if (journalctl.status === 0) run('journalctl', ['--verify', '--file', journalPath]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'chain-online-reopen-0' }]);
    first.append([{ name: 'MESSAGE', value: 'chain-online-reopen-1' }]);
    const activePath = first.activeFile();
    first.writer.close();
    first.writer = null;
    first.closed = true;

    const second = new Log(tempDir, { ...options, headSeqnum: 99 });
    assert.equal(second.activeFile(), activePath);
    assert.notEqual(second.writer, null);
    assert.equal(second.nextSeqnum, 3n);
    second.append([{ name: 'MESSAGE', value: 'chain-online-reopen-2' }]);
    second.close();

    const reader = FileReader.open(activePath);
    const seqnums = [];
    while (reader.step()) seqnums.push(reader.getEntry().seqnum);
    reader.close();
    assert.deepEqual(seqnums, [1n, 2n, 3n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'strict-migrate-0' }], { realtimeUsec: 1700002271000000n, monotonicUsec: 1n });
    first.append([{ name: 'MESSAGE', value: 'strict-migrate-1' }], { realtimeUsec: 1700002271000001n, monotonicUsec: 2n });
    const chainPath = first.activeFile();
    first.writer.close();
    first.writer = null;
    first.closed = true;

    const strict = new Log(tempDir, { ...options, strictSystemdNaming: true });
    const chainReader = FileReader.open(chainPath);
    assert.equal(chainReader.header.state, STATE_ARCHIVED);
    chainReader.close();

    strict.append([{ name: 'MESSAGE', value: 'strict-migrate-2' }], { realtimeUsec: 1700002271000002n, monotonicUsec: 3n });
    assert.equal(strict.activeFile(), join(strict.journalDirectory(), 'system.journal'));
    const activeReader = FileReader.open(strict.activeFile());
    assert.equal(activeReader.header.head_entry_seqnum, 3n);
    assert.equal(activeReader.header.tail_entry_seqnum, 3n);
    activeReader.close();
    strict.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'replace-chain-0' }], { realtimeUsec: 1700002272000000n, monotonicUsec: 1n });
    first.append([{ name: 'MESSAGE', value: 'replace-chain-1' }], { realtimeUsec: 1700002272000001n, monotonicUsec: 2n });
    const activePath = first.activeFile();
    first.writer.close();
    first.writer = null;
    first.closed = true;

    clearKeyedHashFlag(activePath);
    assert.throws(() => Writer.open(activePath), /keyed hash required/);

    const second = new Log(tempDir, options);
    assert.equal(existsSync(activePath), false);
    assert.equal(disposedJournalFiles(second.journalDirectory()).length, 1);
    second.append([{ name: 'MESSAGE', value: 'replace-chain-2' }], { realtimeUsec: 1700002272000002n, monotonicUsec: 3n });
    assert.notEqual(second.activeFile(), activePath);
    const reader = FileReader.open(second.activeFile());
    assert.equal(reader.header.head_entry_seqnum, 3n);
    assert.equal(reader.header.tail_entry_seqnum, 3n);
    reader.close();
    second.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      strictSystemdNaming: true,
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'replace-strict-0' }], { realtimeUsec: 1700002273000000n, monotonicUsec: 1n });
    first.append([{ name: 'MESSAGE', value: 'replace-strict-1' }], { realtimeUsec: 1700002273000001n, monotonicUsec: 2n });
    const activePath = first.activeFile();
    first.writer.close();
    first.writer = null;
    first.closed = true;

    writeHeaderSize(activePath, HEADER_SIZE - 8);
    assert.throws(() => Writer.open(activePath), /outdated header/);

    const second = new Log(tempDir, options);
    assert.equal(existsSync(activePath), false);
    assert.equal(disposedJournalFiles(second.journalDirectory()).length, 1);
    second.append([{ name: 'MESSAGE', value: 'replace-strict-2' }], { realtimeUsec: 1700002273000002n, monotonicUsec: 3n });
    assert.equal(second.activeFile(), join(second.journalDirectory(), 'system.journal'));
    const reader = FileReader.open(second.activeFile());
    assert.equal(reader.header.head_entry_seqnum, 3n);
    assert.equal(reader.header.tail_entry_seqnum, 3n);
    reader.close();
    second.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'empty-reopen-0' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
    first.append([{ name: 'MESSAGE', value: 'empty-reopen-1' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
    first.close();

    const journalDir = first.journalDirectory();
    const names = readdirSync(journalDir).filter((name) => name.endsWith('.journal')).sort();
    assert.equal(names.length, 1);
    const reader = FileReader.open(join(journalDir, names[0]));
    const seqnumId = Buffer.from(reader.header.seqnum_id);
    const nextSeqnum = reader.header.tail_entry_seqnum + 1n;
    reader.close();

    const emptyPath = join(
      journalDir,
      `system@${uuidToString(seqnumId)}-${nextSeqnum.toString(16).padStart(16, '0')}-00060a24181e040a.journal`,
    );
    const empty = Writer.create(emptyPath, {
      machineId: options.machineId,
      seqnumId,
      headSeqnum: nextSeqnum,
    });
    empty.close();

    const second = new Log(tempDir, options);
    second.append([{ name: 'MESSAGE', value: 'empty-reopen-2' }, { name: 'TEST_ID', value: 'node-empty-online-reopen' }]);
    second.close();
    assert.equal(existsSync(emptyPath), false);

    const seqnums = [];
    for (const name of readdirSync(journalDir).filter((name) => name.endsWith('.journal')).sort()) {
      const fileReader = FileReader.open(join(journalDir, name));
      while (fileReader.step()) seqnums.push(fileReader.getEntry().seqnum);
      fileReader.close();
    }
    assert.deepEqual(seqnums, [1n, 2n, 3n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const unlockedPath = join(tempDir, 'writer-unlocked-default.journal');
    const unlocked = Writer.create(unlockedPath);
    unlocked.close();
    assert.equal(existsSync(`${unlockedPath}.lock`), false);

    const journalPath = join(tempDir, 'writer-lock.journal');
    const lock = WriterLock.acquire(journalPath);
    const writer = Writer.create(journalPath);
    try {
      writer.append([{ name: 'MESSAGE', value: 'held' }]);
      assert.ok(existsSync(`${journalPath}.lock`));
      assert.throws(() => WriterLock.acquire(journalPath), /journal writer lock held/);
    } finally {
      writer.close();
      lock.release();
    }
    assert.equal(existsSync(`${journalPath}.lock`), false);
    const reopened = Writer.open(journalPath);
    reopened.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const value = Buffer.from('cafe\u0301', 'utf8');
  const entry = {
    fields: { MESSAGE: value },
    fieldValues: { MESSAGE: [value] },
  };
  assert.deepEqual(jsonEntry(entry).MESSAGE, value.toString('utf8'));
  assert.match(exportEntry(entry).toString('utf8'), /^MESSAGE=cafe\u0301\n\n$/u);
}

{
  const value = Buffer.from([0xff, 0xfe, 0xfd]);
  const entry = {
    fields: { FIELD: value },
    fieldValues: { FIELD: [value] },
  };
  const output = exportEntry(entry);
  const marker = Buffer.from('FIELD\n', 'utf8');
  const markerOffset = output.indexOf(marker);
  assert.notEqual(markerOffset, -1);
  const sizeOffset = markerOffset + marker.length;
  assert.equal(output.readBigUInt64LE(sizeOffset), BigInt(value.length));
  assert.deepEqual(output.subarray(sizeOffset + 8, sizeOffset + 8 + value.length), value);
}

{
  const value = Buffer.from('line1\nline2', 'utf8');
  const entry = {
    fields: { FIELD: value },
    fieldValues: { FIELD: [value] },
  };
  const output = exportEntry(entry);
  const marker = Buffer.from('FIELD\n', 'utf8');
  const markerOffset = output.indexOf(marker);
  assert.notEqual(markerOffset, -1);
  const sizeOffset = markerOffset + marker.length;
  assert.equal(output.readBigUInt64LE(sizeOffset), BigInt(value.length));
  assert.deepEqual(output.subarray(sizeOffset + 8, sizeOffset + 8 + value.length), value);
}

{
  const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 3);
  const unsupportedFlag = 0x80;
  writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, unsupportedFlag, BigInt(DATA_OBJECT_HEADER_SIZE + 3));
  Buffer.from('A=x').copy(buf, DATA_OBJECT_HEADER_SIZE);
  assert.throws(() => parseDataObject(buf, 0), /unsupported DATA object flags/);
}

{
  const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + 3);
  writeObjectHeader(
    buf,
    0,
    OBJECT_TYPE_DATA,
    OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_XZ,
    BigInt(DATA_OBJECT_HEADER_SIZE + 3),
  );
  Buffer.from('A=x').copy(buf, DATA_OBJECT_HEADER_SIZE);
  assert.throws(() => parseDataObject(buf, 0), /unsupported DATA object compression flags/);
}

{
  const payload = Buffer.from(`MESSAGE=${'lz4-data-object'.repeat(16)}`);
  const compressed = compressLz4DataPayload(payload);
  assert.ok(compressed);
  const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + compressed.length);
  writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_LZ4, BigInt(DATA_OBJECT_HEADER_SIZE + compressed.length));
  compressed.copy(buf, DATA_OBJECT_HEADER_SIZE);
  const parsed = parseDataObject(buf, 0);
  assert.deepEqual(parsed.name, Buffer.from('MESSAGE'));
  assert.deepEqual(parsed.value, Buffer.from('lz4-data-object'.repeat(16)));
}

{
  const payload = Buffer.from(`MESSAGE=${'xz-data-object'.repeat(16)}`);
  const compressed = compressXzDataPayload(payload);
  assert.ok(compressed);
  const buf = Buffer.alloc(DATA_OBJECT_HEADER_SIZE + compressed.length);
  writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_XZ, BigInt(DATA_OBJECT_HEADER_SIZE + compressed.length));
  compressed.copy(buf, DATA_OBJECT_HEADER_SIZE);
  const parsed = parseDataObject(buf, 0);
  assert.deepEqual(parsed.name, Buffer.from('MESSAGE'));
  assert.deepEqual(parsed.value, Buffer.from('xz-data-object'.repeat(16)));
}

{
  // XZ CHECK_NONE: verify the emitted stream header uses check type 0 (None).
  const payload = Buffer.from(`MESSAGE=${'xz-check-none-test'.repeat(16)}`);
  const compressed = compressXzDataPayload(payload);
  assert.ok(compressed);
  // systemd XZ magic: fd 37 7a 58 5a 00 (6 bytes), then stream flags (2 bytes).
  assert.equal(compressed.subarray(0, 6).toString('hex'), 'fd377a585a00');
  assert.equal(compressed[6], 0x00, 'XZ stream flag byte 0 must be zero');
  assert.equal(compressed[7], 0x00, 'XZ stream must use CHECK_NONE (0)');
}

{
  // XZ rejects payloads below minimum compression threshold (80 bytes).
  const smallPayload = Buffer.from('short');
  assert.equal(compressXzDataPayload(smallPayload), null);
}

{
  // XZ decompression rejects corrupt/invalid payloads.
  const corrupt = Buffer.alloc(32, 0xff);
  assert.throws(() => decompressXzDataPayload(corrupt), /xz decompression/);
}

{
  // Verify the supported runtime path does not load native addons.
  const req = createRequire(import.meta.url);
  const nativeAddonKeys = Object.keys(req.cache).filter(
    (k) => k.startsWith(packageRoot) && k.endsWith('.node'),
  );
  assert.equal(nativeAddonKeys.length, 0, 'SDK runtime path must not load .node native addons');
}

{
  // Verify production dependencies do not require native install hooks.
  const lock = JSON.parse(readFileSync(join(packageRoot, 'package-lock.json'), 'utf8'));
  const packages = Object.entries(lock.packages ?? {});
  const installScriptPackages = packages
    .filter(([, metadata]) => metadata.hasInstallScript === true)
    .map(([name]) => name);
  assert.deepEqual(installScriptPackages, [], 'package-lock must not include install-script dependencies');
  assert.equal(lock.packages?.['node_modules/node-liblzma'], undefined, 'full node-liblzma package must not be a dependency');
  assert.equal(lock.packages?.['node_modules/node-gyp-build'], undefined, 'node-gyp-build must not be a dependency');
}

{
  // Keep vendored WASM provenance executable, not only documented.
  const expected = new Map([
    ['liblzma.js', 'f33997f0c680a29fd307d18b8336325949811c78bb00ad9a038bf8f205623e02'],
    ['liblzma.wasm', 'a9216b509c9bf0006f306e85f696bd67d31e4ca1972b9e35307aef8650fe705c'],
    ['LICENSE', 'f97bc4bb9b7ae8a653941073678b5c7775e8de44a01c3bcc21e7cdc148b90e61'],
  ]);
  const vendorDir = join(packageRoot, 'vendor/node-liblzma-wasm');
  for (const [fileName, expectedHash] of expected) {
    const actualHash = createHash('sha256')
      .update(readFileSync(join(vendorDir, fileName)))
      .digest('hex');
    assert.equal(actualHash, expectedHash, `${fileName} vendor hash mismatch`);
  }
}

{
  // Node writer -> Node reader round-trip with XZ-compressed DATA.
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'xz-writer-reader-roundtrip.journal');
    const writer = Writer.create(journalPath, { compression: 'xz', compressionThresholdBytes: 80 });
    const value = 'xz-roundtrip-test-value-'.repeat(16);
    writer.append([{ name: 'MESSAGE', value }]);
    writer.close();
    assert.ok(
      journalHasDataObjectFlag(journalPath, OBJECT_COMPRESSED_XZ),
      'journal must contain at least one XZ-compressed DATA object',
    );
    const reader = FileReader.open(journalPath);
    assert.ok(reader.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ, 'header must have XZ incompatible flag');
    assert.ok(reader.next());
    const entry = reader.getEntry();
    assert.ok(entry);
    assert.deepEqual(entry.fields.MESSAGE, Buffer.from(value));
    assert.equal(reader.next(), false);
    reader.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  // Compression threshold policy follows systemd: default 512 bytes, minimum 8 bytes.
  for (const testCase of [
    {
      name: 'default below threshold',
      options: {},
      payloadLength: DEFAULT_COMPRESS_THRESHOLD - 1,
      wantThreshold: DEFAULT_COMPRESS_THRESHOLD,
      wantCompressed: false,
    },
    {
      name: 'default exact threshold',
      options: {},
      payloadLength: DEFAULT_COMPRESS_THRESHOLD,
      wantThreshold: DEFAULT_COMPRESS_THRESHOLD,
      wantCompressed: true,
    },
    {
      name: 'minimum clamp',
      options: { compressionThresholdBytes: 1 },
      payloadLength: MIN_COMPRESS_THRESHOLD - 1,
      wantThreshold: MIN_COMPRESS_THRESHOLD,
      wantCompressed: false,
    },
    {
      name: 'minimum clamp eligible payload',
      options: { compressionThresholdBytes: 1 },
      payloadLength: DEFAULT_COMPRESS_THRESHOLD,
      wantThreshold: MIN_COMPRESS_THRESHOLD,
      wantCompressed: true,
    },
  ]) {
    const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
    try {
      const journalPath = join(tempDir, `zstd-threshold-${testCase.name.replaceAll(' ', '-')}.journal`);
      const writer = Writer.create(journalPath, { compression: 'zstd', ...testCase.options });
      assert.equal(writer.compressThreshold, testCase.wantThreshold);
      writer.append([{ name: 'F', value: Buffer.alloc(testCase.payloadLength - 2, 0x41) }]);
      writer.close();
      assert.equal(
        journalHasDataObjectFlag(journalPath, OBJECT_COMPRESSED_ZSTD),
        testCase.wantCompressed,
        testCase.name,
      );
      verifyJournalFileIfAvailable(journalPath);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'binary-unique.journal');
    const writer = Writer.create(journalPath);
    writer.append([{ name: 'FIELD', value: Buffer.from([0xff]) }]);
    writer.append([{ name: 'FIELD', value: Buffer.from([0xef, 0xbf, 0xbd]) }]);
    writer.close();

    const reader = FileReader.open(journalPath);
    const values = reader.queryUnique('FIELD');
    reader.close();
    assert.equal(values.length, 2);

    const journal = SdJournalOpen(journalPath, 0);
    const facadeValues = SdJournalQueryUnique(journal, 'FIELD');
    journal.close();
    assert.equal(facadeValues.length, 2);
    assert.ok(facadeValues.some(([, value]) => value.equals(Buffer.from([0xff]))));
    assert.ok(facadeValues.some(([, value]) => value.equals(Buffer.from([0xef, 0xbf, 0xbd]))));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'indexed-unique.journal');
    const writer = Writer.create(journalPath);
    for (const priority of ['0', '3', '6', '7']) {
      writer.append([
        { name: 'MESSAGE', value: 'irrelevant' },
        { name: 'PRIORITY', value: priority },
      ]);
    }
    writer.close();

    const reader = FileReader.open(journalPath);
    reader.entryOffsets = [];
    const fields = reader.enumerateFields();
    const values = reader.queryUnique('PRIORITY');
    reader.close();
    assert.ok(fields.has('MESSAGE'));
    assert.ok(fields.has('PRIORITY'));
    assert.deepEqual(
      new Set(values.map(v => v.toString())),
      new Set(['0', '3', '6', '7']),
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const firstPath = join(tempDir, 'unique-first.journal');
    const secondPath = join(tempDir, 'unique-second.journal');
    const first = Writer.create(firstPath);
    first.append([
      { name: 'MESSAGE', value: 'first' },
      { name: 'PRIORITY', value: '6' },
    ]);
    first.close();
    const second = Writer.create(secondPath);
    second.append([
      { name: 'MESSAGE', value: 'second' },
      { name: 'PRIORITY', value: '6' },
    ]);
    second.append([
      { name: 'MESSAGE', value: 'third' },
      { name: 'PRIORITY', value: '3' },
    ]);
    second.close();

    const reader = DirectoryReader.openFiles([firstPath, secondPath]);
    const values = reader.queryUnique('PRIORITY');
    reader.close();
    assert.deepEqual(
      new Set(values.map(v => v.toString())),
      new Set(['3', '6']),
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const journalPath = join(tempDir, 'flags.journal');
    const writer = Writer.create(journalPath);
    writer.append([{ name: 'MESSAGE', value: 'flags' }]);
    writer.close();

    assert.throws(() => SdJournalOpen(journalPath, 1), /unsupported sd_journal_open flags/);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'custom-source',
      maxEntries: 2,
      maxBytes: 16 * 1024 * 1024,
      maxFiles: 10,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
    });
    for (let i = 0; i < 5; i++) {
      log.append([{ name: 'MESSAGE', value: `entry-${i}` }]);
      if (i === 0) {
        assert.match(basename(log.activeFile()), /^custom-source@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
        assert.equal(existsSync(join(log.journalDirectory(), 'custom-source.journal')), false);
      }
    }
    log.close();
    assert.throws(() => log.append([{ name: 'MESSAGE', value: 'after-close' }]), /journal log is closed/);

    assert.equal(log.journalDirectory(), join(tempDir, '00112233445566778899aabbccddeeff'));
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal') || name.endsWith('.journal~')).sort();
    const archived = files.filter((name) => name.endsWith('.journal~'));
    assert.equal(archived.length, 0);
    assert.equal(files.length, 3);
    assert.ok(files.every((name) => /^custom-source@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/.test(name)));

    const seqnums = [];
    for (const name of files) {
      const reader = FileReader.open(join(log.journalDirectory(), name));
      assert.equal(reader.header.state, STATE_ARCHIVED);
      while (reader.step()) {
        seqnums.push(reader.getEntry().seqnum);
      }
      reader.close();
    }
    assert.deepEqual(seqnums, [1n, 2n, 3n, 4n, 5n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const bootId = '0123456789abcdef0123456789abcdef';
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      bootId: Buffer.from(bootId, 'hex'),
    });
    log.append([
      { name: 'MESSAGE', value: 'journald policy preserves trusted fields' },
      { name: 'TEST_ID', value: 'journald-field-policy' },
      { name: '_HOSTNAME', value: 'synthetic-host' },
      { name: '_TRANSPORT', value: 'snmptrap' },
      { name: '_BOOT_ID', value: bootId },
    ], { realtimeUsec: 1_700_002_401_000_000n, monotonicUsec: 10n });
    log.sync();

    const reader = FileReader.open(log.activeFilePath());
    const entries = [];
    try {
      while (reader.step()) entries.push(reader.getEntry());
    } finally {
      reader.close();
    }
    assert.equal(entries.length, 1);
    assert.equal(entries[0].fields._HOSTNAME.toString('utf8'), 'synthetic-host');
    assert.equal(entries[0].fields._TRANSPORT.toString('utf8'), 'snmptrap');
    assert.equal(entries[0].fields._BOOT_ID.toString('utf8'), bootId);
    assert.equal(entries[0].fieldValues._BOOT_ID.length, 1);

    const journalctl = spawnSync('journalctl', ['--version'], { encoding: 'utf8' });
    if (journalctl.status === 0) {
      const stockRows = run('journalctl', ['--directory', log.journalDirectory(), '--output=json', '--no-pager', 'TEST_ID=journald-field-policy'])
        .trim()
        .split('\n')
        .filter(Boolean)
        .map((line) => JSON.parse(line));
      assert.equal(stockRows.length, 1);
      assert.equal(stockRows[0]._HOSTNAME, 'synthetic-host');
      assert.equal(stockRows[0]._TRANSPORT, 'snmptrap');
      assert.equal(stockRows[0]._BOOT_ID, bootId);
    }
    log.close();
    for (const path of journalFiles(log.journalDirectory())) verifyJournalFileIfAvailable(path);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
    });
    log.append([
      { name: 'MESSAGE', value: 'journal app keeps valid fields' },
      { name: 'TEST_ID', value: 'journal-app-field-policy' },
      { name: '_HOSTNAME', value: 'dropped-host' },
      { name: 'foo.bar', value: 'dropped-dot' },
    ], {
      realtimeUsec: 1_700_002_402_000_000n,
      monotonicUsec: 20n,
    });
    assert.throws(
      () => log.append([{ name: '_HOSTNAME', value: 'drop-only' }], {
        realtimeUsec: 1_700_002_402_000_001n,
        monotonicUsec: 21n,
      }),
      /empty entry/,
    );
    log.close();

    const path = journalFiles(log.journalDirectory())[0];
    const reader = FileReader.open(path);
    const entries = [];
    try {
      while (reader.step()) entries.push(reader.getEntry());
    } finally {
      reader.close();
    }
    assert.equal(entries.length, 1);
    assert.equal(entries[0].fields.MESSAGE.toString('utf8'), 'journal app keeps valid fields');
    assert.match(entries[0].fields._BOOT_ID.toString('utf8'), /^[0-9a-f]{32}$/);
    assert.equal(entries[0].fields._HOSTNAME, undefined);
    assert.equal(entries[0].fields['foo.bar'], undefined);
    verifyJournalFileIfAvailable(path);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const bootId = '0123456789abcdef0123456789abcdef';
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      bootId: Buffer.from(bootId, 'hex'),
      fieldNamePolicy: FIELD_NAME_POLICY_JOURNAL_APP,
    });
    log.appendRaw([
      Buffer.from('MESSAGE=raw directory'),
      Buffer.from('TEST_ID=node-log-append-raw'),
      Buffer.from('_HOSTNAME=drop-host'),
      Buffer.from('lowercase=drop-lowercase'),
    ], {
      realtimeUsec: 1_700_002_404_000_000n,
      monotonicUsec: 40n,
      sourceRealtimeUsec: 1234n,
    });
    assert.throws(
      () => log.appendRaw([Buffer.from('_HOSTNAME=drop-only')], {
        realtimeUsec: 1_700_002_404_000_001n,
        monotonicUsec: 41n,
      }),
      /empty entry/,
    );
    const journalDir = log.journalDirectory();
    log.close();

    const path = journalFiles(journalDir)[0];
    const reader = FileReader.open(path);
    assert.equal(reader.step(), true);
    const entry = reader.getEntry();
    reader.close();
    assert.equal(entry.fields.MESSAGE.toString('utf8'), 'raw directory');
    assert.equal(entry.fields.TEST_ID.toString('utf8'), 'node-log-append-raw');
    assert.equal(entry.fields._BOOT_ID.toString('utf8'), bootId);
    assert.equal(entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8'), '1234');
    assert.equal(entry.fields._HOSTNAME, undefined);
    assert.equal(entry.fields.lowercase, undefined);

    const rows = journalctlDirectoryRowsIfAvailable(
      journalDir,
      `_BOOT_ID=${bootId}`,
      'TEST_ID=node-log-append-raw',
    );
    if (rows !== null) {
      assert.equal(rows.length, 1);
      assert.equal(rows[0].MESSAGE, 'raw directory');
    }
    verifyJournalFileIfAvailable(path);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const longName = 'a'.repeat(1024);
    const log = new Log(tempDir, {
      source: 'system',
      fieldNamePolicy: FIELD_NAME_POLICY_RAW,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
    });
    log.append([
      { name: 'lowercase', value: 'ok' },
      { name: 'foo.bar', value: 'dot' },
      { name: 'field name', value: 'space' },
      { name: longName, value: 'long' },
      { name: 'BINARY', value: Buffer.from([0x61, 0x00, 0x3d, 0x62]) },
    ], {
      realtimeUsec: 1_700_002_403_000_000n,
      monotonicUsec: 30n,
    });
    assert.throws(
      () => log.append([{ name: 'BAD=NAME', value: 'bad' }], {
        realtimeUsec: 1_700_002_403_000_001n,
        monotonicUsec: 31n,
      }),
      /invalid field name/,
    );
    log.close();

    const path = journalFiles(log.journalDirectory())[0];
    const reader = FileReader.open(path);
    const entries = [];
    try {
      while (reader.step()) entries.push(reader.getEntry());
    } finally {
      reader.close();
    }
    assert.equal(entries.length, 1);
    assert.equal(entries[0].fields.lowercase.toString('utf8'), 'ok');
    assert.equal(entries[0].fields['foo.bar'].toString('utf8'), 'dot');
    assert.equal(entries[0].fields['field name'].toString('utf8'), 'space');
    assert.equal(entries[0].fields[longName].toString('utf8'), 'long');
    assert.deepEqual(entries[0].fields.BINARY, Buffer.from([0x61, 0x00, 0x3d, 0x62]));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxDurationUsec: 10_000_000n,
      maxFiles: 10,
    });
    const base = 1_700_002_090_000_000n;
    for (const [i, realtime] of [base, base + 9_999_999n, base + 10_000_000n].entries()) {
      log.append(
        [{ name: 'MESSAGE', value: `duration-rotation-${i}` }],
        { realtimeUsec: realtime, monotonicUsec: BigInt(i + 1) },
      );
    }
    log.close();
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
    assert.equal(files.length, 2);
    const counts = files.map((name) => {
      const reader = FileReader.open(join(log.journalDirectory(), name));
      try {
        return reader.header.n_entries;
      } finally {
        reader.close();
      }
    });
    assert.deepEqual(counts, [2n, 1n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    });
    log.append([{ name: 'MESSAGE', value: 'default system naming' }]);
    assert.equal(log.nextSeqnum, 2n);
    assert.match(basename(log.activeFile()), /^system@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
    assert.equal(existsSync(join(log.journalDirectory(), 'system.journal')), false);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const baseOptions = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 0,
    };
    const first = new Log(tempDir, baseOptions);
    for (let i = 0; i < 3; i++) {
      first.append(
        [{ name: 'MESSAGE', value: `age-retention-${i}` }],
        { realtimeUsec: BigInt(1_000_000 + i), monotonicUsec: BigInt(i + 1) },
      );
    }
    first.close();
    const journalDir = first.journalDirectory();
    assert.equal(readdirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 3);

    const retained = new Log(tempDir, { ...baseOptions, maxEntries: 0, maxRetentionAgeUsec: 1_000_000n });
    assert.equal(readdirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 3);
    retained.enforceRetention();
    assert.equal(readdirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 0);
    retained.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const baseOptions = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 0,
    };
    const first = new Log(tempDir, baseOptions);
    for (let i = 0; i < 2; i++) {
      first.append(
        [{ name: 'MESSAGE', value: `age-active-retention-${i}` }],
        { realtimeUsec: BigInt(1_000_000 + i), monotonicUsec: BigInt(i + 1) },
      );
    }
    first.close();
    const journalDir = first.journalDirectory();
    assert.equal(readdirSync(journalDir).filter((name) => name.endsWith('.journal')).length, 2);

    const retained = new Log(tempDir, { ...baseOptions, maxEntries: 0, maxRetentionAgeUsec: 1_000_000n });
    retained.append(
      [{ name: 'MESSAGE', value: 'age-protected-active' }],
      { realtimeUsec: 1_000_100n, monotonicUsec: 10n },
    );
    const activePath = retained.activeFile();
    retained.enforceRetention();
    const files = readdirSync(journalDir).filter((name) => name.endsWith('.journal')).sort();
    assert.deepEqual(files.map((name) => join(journalDir, name)), [activePath]);
    const reader = FileReader.open(activePath);
    assert.equal(reader.step(), true);
    assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'age-protected-active');
    reader.close();
    retained.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    });
    assert.throws(() => log.append([]), /empty entry/);
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
    assert.equal(files.length, 0);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const config = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 0,
    };
    const first = new Log(tempDir, config);
    for (let i = 0; i < 2; i++) {
      first.append([{ name: 'MESSAGE', value: `construction-retention-${i}` }]);
    }
    first.close();
    const journalDir = first.journalDirectory();
    const before = journalFiles(journalDir);
    assert.equal(before.length, 2);

    const events = [];
    const second = new Log(tempDir, { ...config, maxFiles: 1, lifecycle: (event) => events.push(event) });
    const after = journalFiles(journalDir);
    assert.deepEqual(after, before);
    second.append([
      { name: 'MESSAGE', value: 'construction-retention-open' },
      { name: 'TEST_ID', value: 'node-retention-on-open' },
    ]);
    const afterAppend = journalFiles(journalDir);
    assert.deepEqual(afterAppend, [second.activeFile()]);
    assert.ok(events.some((event) => event.type === 'deleted'));
    verifyJournalFileIfAvailable(second.activeFile());
    const rows = journalctlDirectoryRowsIfAvailable(journalDir, 'TEST_ID=node-retention-on-open');
    if (rows !== null) assert.equal(rows.length, 1);
    second.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

for (const retentionCase of [
  { name: 'files', options: { maxFiles: 1 } },
  { name: 'bytes', options: { maxRetentionBytes: 1 }, artifact: true },
  { name: 'age', options: { maxRetentionAgeUsec: 1n } },
]) {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const config = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 0,
    };
    const first = new Log(tempDir, config);
    for (let i = 0; i < 3; i++) {
      first.append(
        [{ name: 'MESSAGE', value: `open-retention-${retentionCase.name}-${i}` }],
        { realtimeUsec: BigInt(1_700_002_276_000_000 + i), monotonicUsec: BigInt(i + 1) },
      );
    }
    first.close();
    const journalDir = first.journalDirectory();
    assert.equal(journalFiles(journalDir).length, 3);

    const events = [];
    const artifactCalls = [];
    const retained = new Log(tempDir, {
      ...config,
      maxEntries: 0,
      openMode: 'eager',
      ...retentionCase.options,
      lifecycle: (event) => events.push(event),
      artifactSizer: retentionCase.artifact
        ? (path) => {
          artifactCalls.push(path);
          return 4096;
        }
        : undefined,
    });
    const files = journalFiles(journalDir);
    assert.deepEqual(files, [retained.activeFile()]);
    assert.ok(events.some((event) => event.type === 'created' && event.reason === 'eager_open'));
    assert.ok(events.some((event) => event.type === 'deleted'));
    if (retentionCase.artifact) assert.ok(artifactCalls.length > 0);
    verifyJournalFileIfAvailable(retained.activeFile());
    retained.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      strictSystemdNaming: true,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 100,
      maxFiles: 10,
    });
    log.append([{ name: 'MESSAGE', value: 'strict naming' }]);
    assert.equal(log.activeFile(), join(log.journalDirectory(), 'system.journal'));
    log.close();
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
    assert.equal(files.length, 1);
    assert.match(files[0], /^system@[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{16}\.journal$/);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 1,
      maxRetentionBytes: 1024 * 1024 * 1024,
    });
    for (let i = 0; i < 3; i++) {
      log.append([{ name: 'MESSAGE', value: `retention-active-${i}` }]);
    }
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
    assert.equal(files.length, 1);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
    });
    assert.equal(log.bootID().length, 16);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const machineId = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
    const bootId = Buffer.from('ffeeddccbbaa99887766554433221100', 'hex');
    assert.throws(
      () => new Log(tempDir, { identityMode: 'strict', machineId }),
      /strict identity requires boot id/,
    );

    const events = [];
    const log = new Log(tempDir, {
      source: 'system',
      openMode: 'eager',
      identityMode: 'strict',
      machineId,
      bootId,
      lifecycle: (event) => events.push(event),
    });
    assert.equal(log.configuredDirectory(), tempDir);
    assert.equal(log.journalDirectory(), join(tempDir, '00112233445566778899aabbccddeeff'));
    assert.equal(log.machineID().toString('hex'), machineId.toString('hex'));
    assert.equal(log.bootID().toString('hex'), bootId.toString('hex'));
    assert.equal(log.sourceName(), 'system');
    assert.notEqual(log.activeFilePath(), '');
    assert.equal(events.length, 1);
    assert.equal(events[0].type, 'created');
    assert.equal(events[0].reason, 'eager_open');
    assert.equal(events[0].activePath, log.activeFilePath());

    log.append(
      [{ name: 'MESSAGE', value: 'timestamp-0' }],
      { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 10n, sourceRealtimeUsec: 999n },
    );
    log.append(
      [{ name: 'MESSAGE', value: 'timestamp-1' }],
      { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 10n, sourceRealtimeUsec: 1000n },
    );
    log.append(
      [{ name: 'MESSAGE', value: 'timestamp-2' }],
      { realtimeUsec: 1_700_000_100_000_000n, monotonicUsec: 0n, sourceRealtimeUsec: 1001n },
    );
    log.append(
      [{ name: 'MESSAGE', value: 'timestamp-3' }],
      { realtimeUsec: 0n, monotonicUsec: 13n, sourceRealtimeUsec: 1002n },
    );
    const path = log.activeFilePath();
    log.close();
    verifyJournalFileIfAvailable(path);

    const reader = FileReader.open(path);
    const entries = [];
    while (reader.step()) entries.push(reader.getEntry());
    reader.close();
    assert.equal(entries.length, 4);
    assert.deepEqual(entries.map((entry) => entry.realtime), [
      1_700_000_100_000_000n,
      1_700_000_100_000_001n,
      1_700_000_100_000_002n,
      1_700_000_100_000_003n,
    ]);
    assert.deepEqual(entries.map((entry) => entry.monotonic), [10n, 11n, 12n, 13n]);
    assert.deepEqual(
      entries.map((entry) => entry.fields._SOURCE_REALTIME_TIMESTAMP.toString('utf8')),
      ['999', '1000', '1001', '1002'],
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    assert.throws(() => new Log(tempDir, { rotationPolicy: { maxEntries: 0 } }), /rotation max entries/);
    assert.throws(() => new Log(tempDir, { retentionPolicy: { maxFiles: 0 } }), /retention max files/);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const events = [];
    const artifactCalls = [];
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      retentionPolicy: { maxBytes: 1 },
      lifecycle: (event) => events.push(event),
      artifactSizer: (path) => {
        artifactCalls.push(path);
        return 4096;
      },
    });
    log.append([{ name: 'MESSAGE', value: 'artifact-retention-0' }]);
    log.append([{ name: 'MESSAGE', value: 'artifact-retention-1' }]);
    assert.ok(artifactCalls.length > 0);
    assert.ok(events.some((event) => event.type === 'created' && event.reason === 'append'));
    assert.ok(events.some((event) => event.type === 'rotated'));
    const deleted = events.find((event) => event.type === 'deleted');
    assert.ok(deleted);
    assert.equal(deleted.deletedPaths.length, 1);
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
    assert.equal(files.length, 1);
    log.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      strictSystemdNaming: true,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 100,
      maxFiles: 10,
      maxRetentionBytes: 1,
    });
    log.append([
      { name: 'MESSAGE', value: 'strict byte retained' },
      { name: 'TEST_ID', value: 'node-strict-byte-retention' },
    ]);
    log.close();
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort();
    assert.equal(files.length, 1);
    const reader = FileReader.open(join(log.journalDirectory(), files[0]));
    assert.equal(reader.step(), true);
    assert.equal(reader.getEntry().fields.MESSAGE.toString('utf8'), 'strict byte retained');
    reader.close();
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 100,
      maxFiles: 10,
    });
    log.append([{ name: 'MESSAGE', value: 'archive failure cleanup' }]);
    const originalArchiveTo = log.writer.archiveTo.bind(log.writer);
    log.writer.archiveTo = (path) => {
      originalArchiveTo(path);
      throw new Error('synthetic post-archive failure');
    };
    assert.throws(() => log.close(), /synthetic post-archive failure/);
    assert.equal(log.closed, true);
    assert.equal(log.writer, null);
    assert.doesNotThrow(() => log.close());
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 1,
      maxFiles: 10,
    });
    log.append([{ name: 'MESSAGE', value: 'rotation failure first' }]);
    const originalArchiveTo = log.writer.archiveTo.bind(log.writer);
    log.writer.archiveTo = (path) => {
      originalArchiveTo(path);
      throw new Error('synthetic post-rotation failure');
    };
    assert.throws(
      () => log.append([{ name: 'MESSAGE', value: 'rotation failure second' }]),
      /synthetic post-rotation failure/,
    );
    assert.equal(log.closed, false);
    assert.equal(log.writer, null);
    log.append([{ name: 'MESSAGE', value: 'rotation failure second' }]);
    log.close();

    const seqnums = [];
    for (const name of readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
      const reader = FileReader.open(join(log.journalDirectory(), name));
      while (reader.step()) seqnums.push(reader.getEntry().seqnum);
      reader.close();
    }
    assert.deepEqual(seqnums, [1n, 2n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      strictSystemdNaming: true,
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 100,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'strict-reopen-0' }]);
    first.close();
    const second = new Log(tempDir, options);
    second.append([{ name: 'MESSAGE', value: 'strict-reopen-1' }]);
    second.close();
    const seqnums = [];
    for (const name of readdirSync(second.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
      const reader = FileReader.open(join(second.journalDirectory(), name));
      while (reader.step()) seqnums.push(reader.getEntry().seqnum);
      reader.close();
    }
    assert.deepEqual(seqnums, [1n, 2n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const options = {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    };
    const first = new Log(tempDir, options);
    first.append([{ name: 'MESSAGE', value: 'chain-reopen-0' }]);
    first.append([{ name: 'MESSAGE', value: 'chain-reopen-1' }]);
    first.close();

    const second = new Log(tempDir, options);
    second.append([{ name: 'MESSAGE', value: 'chain-reopen-2' }]);
    second.close();

    const seqnums = [];
    for (const name of readdirSync(second.journalDirectory()).filter((name) => name.endsWith('.journal')).sort()) {
      const reader = FileReader.open(join(second.journalDirectory(), name));
      while (reader.step()) seqnums.push(reader.getEntry().seqnum);
      reader.close();
    }
    assert.deepEqual(seqnums, [1n, 2n, 3n]);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-journal-test-'));
  try {
    const log = new Log(tempDir, {
      source: 'system',
      machineId: Buffer.from('00112233445566778899aabbccddeeff', 'hex'),
      maxEntries: 0,
      maxBytes: 0,
      maxFiles: 10,
    });
    for (let i = 0; i < 3; i++) log.append([{ name: 'MESSAGE', value: `no-rotation-${i}` }]);
    log.close();
    const files = readdirSync(log.journalDirectory()).filter((name) => name.endsWith('.journal'));
    assert.equal(files.length, 1);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

assert.throws(() => new Log('/tmp', { source: '../bad' }), /invalid journal source/);

for (const file of listJavaScriptFiles(packageRoot).sort()) {
  run(process.execPath, ['--check', file], { cwd: repoRoot });
}

run(process.execPath, ['-e', "import './node/src/index.js'"], { cwd: repoRoot });

{
  // FSPRG vector tests against committed systemd v260.1 fixture.
  const vectorsPath = join(repoRoot, 'tests/fss/fixtures/fsprg-vectors-v01.json');
  const vectorsData = JSON.parse(readFileSync(vectorsPath, 'utf8'));
  const secpar = vectorsData.fsprg_params.secpar;
  for (const vec of vectorsData.vectors) {
    const seed = Buffer.from(vec.seed_hex, 'hex');
    const expectedMsk = Buffer.from(vec.msk_hex, 'hex');
    const expectedMpk = Buffer.from(vec.mpk_hex, 'hex');
    const expectedState0 = Buffer.from(vec.state0_hex, 'hex');

    const { msk, mpk } = fsprgGenMK(seed, secpar);
    assert.deepEqual(msk, expectedMsk, `msk mismatch for ${vec.seed_desc}`);
    assert.deepEqual(mpk, expectedMpk, `mpk mismatch for ${vec.seed_desc}`);

    const state0 = fsprgGenState0(mpk, seed);
    assert.deepEqual(state0, expectedState0, `state0 mismatch for ${vec.seed_desc}`);
    assert.equal(fsprgGetEpoch(state0), 0n, `epoch0 mismatch for ${vec.seed_desc}`);

    for (const ep of vec.epochs) {
      let evolved = state0;
      for (let e = 0n; e < BigInt(ep.epoch); e++) {
        evolved = fsprgEvolve(evolved);
      }
      assert.deepEqual(evolved, Buffer.from(ep.state_hex, 'hex'), `evolve mismatch for ${vec.seed_desc} epoch ${ep.epoch}`);

      const seeked = fsprgSeek(state0, BigInt(ep.epoch), msk, seed);
      assert.deepEqual(seeked, Buffer.from(ep.seek_state_hex, 'hex'), `seek mismatch for ${vec.seed_desc} epoch ${ep.epoch}`);

      for (const k of ep.keys) {
        const key = fsprgGetKey(evolved, k.keylen, k.idx);
        assert.deepEqual(key, Buffer.from(k.key_hex, 'hex'), `key mismatch for ${vec.seed_desc} epoch ${ep.epoch} idx ${k.idx}`);
      }
    }
  }
}

// Verification tests
{
  const path = join(repoRoot, 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst');
  try {
    verifyFile(path);
    throw new Error('expected VerificationError for truncated zstd frame');
  } catch (err) {
    if (!(err instanceof VerificationError)) throw new Error(`expected VerificationError, got ${err.constructor.name}`);
    if (!err.message.includes('corrupt')) throw new Error(`expected error to contain 'corrupt', got: ${err.message}`);
  }
}

{
  const path = join(repoRoot, 'fixtures/systemd/test-data/no-rtc/system.journal.zst');
  verifyFile(path); // should not throw
}

// journalctl command verify tests
{
  const validPath = join(repoRoot, 'fixtures/systemd/test-data/no-rtc/system.journal.zst');
  const corruptPath = join(repoRoot, 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst');
  const cmd = process.execPath;
  const script = join(packageRoot, 'cmd/journalctl/index.js');

  // --verify valid file
  {
    const result = spawnSync(cmd, [script, '--verify', '--file', validPath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`--verify valid file failed: stderr=${result.stderr}`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (!result.stderr.includes('PASS:')) {
      throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
    }
  }

  // --verify-only valid file (no normal output)
  {
    const result = spawnSync(cmd, [script, '--verify-only', '--file', validPath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`--verify-only valid file failed: stderr=${result.stderr}`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (!result.stderr.includes('PASS:')) {
      throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
    }
  }

  // --verify directory follows symlinked journals and skips directories
  {
    const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-dir-'));
    symlinkSync(validPath, join(tmpDir, 'linked.journal.zst'));
    mkdirSync(join(tmpDir, 'skip.journal.zst'));

    const result = spawnSync(cmd, [script, '--verify', '--directory', tmpDir], { encoding: 'utf8' });
    rmSync(tmpDir, { recursive: true });
    if (result.status !== 0) {
      throw new Error(`--verify directory failed: stderr=${result.stderr}`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if ((result.stderr.match(/PASS:/g) || []).length !== 1) {
      throw new Error(`expected one PASS in stderr, got: ${result.stderr}`);
    }
    if (result.stderr.includes('FAIL:')) {
      throw new Error(`expected no FAIL in stderr, got: ${result.stderr}`);
    }
  }

  // --verify empty directory
  {
    const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-empty-dir-'));
    const result = spawnSync(cmd, [script, '--verify', '--directory', tmpDir], { encoding: 'utf8' });
    rmSync(tmpDir, { recursive: true });
    if (result.status !== 0) {
      throw new Error(`expected --verify empty directory to succeed: ${result.stderr}`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (result.stderr !== '') {
      throw new Error(`expected no stderr, got: ${result.stderr}`);
    }
  }

  // --verify corrupted file
  {
    const result = spawnSync(cmd, [script, '--verify', '--file', corruptPath], { encoding: 'utf8' });
    if (result.status === 0) {
      throw new Error(`expected --verify corrupted file to fail`);
    }
    if (!result.stderr.includes('FAIL:')) {
      throw new Error(`expected FAIL in stderr, got: ${result.stderr}`);
    }
  }

  // --verify-key unsealed file (valid key parsed, normal verification)
  {
    const result = spawnSync(cmd, [script, '--verify-key', validFSSVerificationKey, '--file', validPath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`--verify-key unsealed file failed: stderr=${result.stderr}`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (!result.stderr.includes('PASS:')) {
      throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
    }
  }

  // --verify-key invalid seed
  {
    const result = spawnSync(cmd, [script, '--verify-key', 'synthetic-test-key', '--file', validPath], { encoding: 'utf8' });
    if (result.status === 0) {
      throw new Error(`expected --verify-key invalid seed to fail`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (!result.stderr.includes('Failed to parse seed.')) {
      throw new Error(`expected parse seed error in stderr, got: ${result.stderr}`);
    }
  }

  // --verify-key empty seed
  {
    const result = spawnSync(cmd, [script, '--verify-key=', '--file', validPath], { encoding: 'utf8' });
    if (result.status === 0) {
      throw new Error(`expected --verify-key empty seed to fail`);
    }
    if (result.stdout !== '') {
      throw new Error(`expected no stdout, got: ${result.stdout}`);
    }
    if (!result.stderr.includes('Failed to parse seed.')) {
      throw new Error(`expected parse seed error in stderr, got: ${result.stderr}`);
    }
  }

  // --verify sealed file without key (key required)
  {
    const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-nokey-'));
    const tmpPath = join(tmpDir, 'sealed.journal');
    const src = readFileSync(validPath);
    const decompressed = decompressZstSync(src);
    const buf = Buffer.from(decompressed);
    const flags = buf.readUInt32LE(8);
    buf.writeUInt32LE(flags | 1, 8); // set COMPATIBLE_SEALED
    writeFileSync(tmpPath, buf);

    const result = spawnSync(cmd, [script, '--verify', '--file', tmpPath], { encoding: 'utf8' });
    rmSync(tmpDir, { recursive: true });
    if (result.status === 0) {
      throw new Error(`expected --verify sealed file without key to fail`);
    }
    if (!result.stderr.includes('verification key')) {
      throw new Error(`expected verification key message in stderr, got: ${result.stderr}`);
    }
    if (result.stderr.includes('PASS:')) {
      throw new Error(`sealed file without key should not pass, got: ${result.stderr}`);
    }
  }

  // --verify-key sealed file (valid)
  {
    const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-valid-'));
    const journalPath = join(tmpDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
    writer.close();
    const key = testVerificationKey(testSealOpts());
    const result = spawnSync(cmd, [script, '--verify-key', key, '--file', journalPath], { encoding: 'utf8' });
    rmSync(tmpDir, { recursive: true });
    if (result.status !== 0) {
      throw new Error(`expected --verify-key sealed file to pass, got: ${result.stderr}`);
    }
    if (!result.stderr.includes('PASS:')) {
      throw new Error(`expected PASS in stderr, got: ${result.stderr}`);
    }
  }

  // --verify-key sealed file wrong key
  {
    const tmpDir = mkdtempSync(join(tmpdir(), 'node-verify-sealed-wrong-'));
    const journalPath = join(tmpDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
    writer.close();
    const wrongKey = '000000000000000000000001/1-f4240';
    const result = spawnSync(cmd, [script, '--verify-key', wrongKey, '--file', journalPath], { encoding: 'utf8' });
    rmSync(tmpDir, { recursive: true });
    if (result.status === 0) {
      throw new Error(`expected --verify-key sealed file with wrong key to fail`);
    }
    if (!result.stderr.includes('FAIL:')) {
      throw new Error(`expected FAIL in stderr, got: ${result.stderr}`);
    }
  }
}

function testSealOpts() {
  return new SealOptions(Buffer.alloc(12), 1_000_000, 1_000_000);
}

function testVerificationKey(opts) {
  const seedHex = opts.seed.toString('hex').padStart(24, '0');
  const start = Math.floor(Number(opts.startUsec) / Number(opts.intervalUsec));
  return `${seedHex}/${start.toString(16)}-${opts.intervalUsec.toString(16)}`;
}

function tamperDataPayload(path, expectedPayload) {
  const buf = Buffer.from(readFileSync(path));
  const headerSize = Number(buf.readBigUInt64LE(88));
  const tailObjectOffset = Number(buf.readBigUInt64LE(136));
  const compact = (buf.readUInt32LE(12) & INCOMPATIBLE_COMPACT) !== 0;
  let offset = headerSize;
  let tagCount = 0;
  let secondTagOffset = 0;
  let targetPayloadOffset = 0;
  let targetObjectOffset = 0;

  while (offset + 16 <= buf.length) {
    const header = parseObjectHeader(buf, offset);
    if (!header || header.size < 16n) throw new Error(`invalid object at ${offset}`);
    const aligned = Number(((header.size + 7n) / 8n) * 8n);
    if (offset + aligned > buf.length) throw new Error(`object at ${offset} exceeds file`);

    if (header.type === OBJECT_TYPE_TAG) {
      tagCount += 1;
      if (tagCount === 2) secondTagOffset = offset;
    } else if (header.type === OBJECT_TYPE_DATA) {
      const payloadOffset = compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
      if (header.size > BigInt(payloadOffset)) {
        const start = offset + payloadOffset;
        const end = offset + Number(header.size);
        if (buf.slice(start, end).equals(expectedPayload)) {
          targetPayloadOffset = start;
          targetObjectOffset = offset;
        }
      }
    }

    if (offset === tailObjectOffset) break;
    offset += aligned;
  }

  if (targetPayloadOffset === 0) throw new Error(`payload not found: ${expectedPayload}`);
  if (secondTagOffset === 0) throw new Error('second TAG not found');
  if (targetObjectOffset >= secondTagOffset) {
    throw new Error(`DATA object ${targetObjectOffset} is not covered by second TAG ${secondTagOffset}`);
  }
  buf[targetPayloadOffset] ^= 0x01;
  writeFileSync(path, buf);
}

// Sealed verification API validates HMACs and keeps structural verification.
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sdk-verify-sealed-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'sealed-covered' }], { realtimeUsec: 1_500_000n });
    writer.append([{ name: 'MESSAGE', value: 'later-entry' }], { realtimeUsec: 2_500_000n });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyFileWithKey(journalPath, key);
    const zstPath = `${journalPath}.zst`;
    writeFileSync(zstPath, zstdCompressSync(readFileSync(journalPath)));
    verifyFileWithKey(zstPath, key);
    assert.throws(
      () => verifyFileWithKey(journalPath, '000000000000000000000001/1-f4240'),
      VerificationError,
    );
    assert.throws(
      () => verifyFileWithKey(journalPath, '000000000000000000000000/10000000000000000-f4240'),
      VerificationError,
    );
    assert.throws(
      () => verifyFileWithKey(journalPath, '000000000000000000000000/1-10000000000000000'),
      VerificationError,
    );

    tamperDataPayload(journalPath, Buffer.from('MESSAGE=sealed-covered'));
    assert.throws(() => verifyFileWithKey(journalPath, key), VerificationError);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer basic
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-basic-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'hello sealed world' }, { name: 'PRIORITY', value: '6' }], {
      realtimeUsec: 1_500_000n,
    });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer interval crossing
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-interval-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'epoch0' }], { realtimeUsec: 1_000_000n });
    writer.append([{ name: 'MESSAGE', value: 'epoch1' }], { realtimeUsec: 2_000_000n });
    writer.append([{ name: 'MESSAGE', value: 'epoch2' }], { realtimeUsec: 3_000_000n });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Unsealed writer does not set sealed flags
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-unsealed-flags-'));
  try {
    const journalPath = join(tempDir, 'unsealed.journal');
    const writer = Writer.create(journalPath);
    writer.append([{ name: 'MESSAGE', value: 'unsealed' }]);
    writer.close();
    const buf = readFileSync(journalPath);
    assert.equal(buf.length >= 16, true);
    const compatibleFlags = buf.readUInt32LE(8);
    if (compatibleFlags & COMPATIBLE_SEALED) {
      throw new Error('unsealed writer set COMPATIBLE_SEALED flag');
    }
    if (compatibleFlags & COMPATIBLE_SEALED_CONTINUOUS) {
      throw new Error('unsealed writer set COMPATIBLE_SEALED_CONTINUOUS flag');
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer first entry in a future epoch
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-first-future-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'future epoch first entry' }], {
      realtimeUsec: 3_000_000n,
    });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify first-entry future-epoch');
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer rejects entries before the configured sealing start
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-before-start-'));
  let writer;
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    writer = Writer.create(journalPath, { seal: testSealOpts() });
    let rejected = false;
    try {
      writer.append([{ name: 'MESSAGE', value: 'before sealing start' }], {
        realtimeUsec: 500_000n,
      });
    } catch (_) {
      rejected = true;
    }
    if (!rejected) {
      throw new Error('expected before-start entry to be rejected');
    }
    writer.close();
  } finally {
    if (writer && !writer.closed) writer.close();
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer handles a multi-interval epoch gap
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-multi-gap-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'epoch0' }], { realtimeUsec: 1_000_000n });
    writer.append([{ name: 'MESSAGE', value: 'epoch5' }], { realtimeUsec: 6_000_000n });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify multi-interval gap');
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Empty sealed writer produces a stock-verifiable file
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-empty-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify empty sealed file');
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Compact sealed writer passes stock journalctl verify
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-compact-sealed-'));
  try {
    const journalPath = join(tempDir, 'compact-sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts(), compact: true });
    writer.append([{ name: 'MESSAGE', value: 'compact sealed' }, { name: 'PRIORITY', value: '6' }], {
      realtimeUsec: 1_500_000n,
    });
    writer.close();

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyIfAvailable(journalPath, key, 'journalctl verify compact+sealed');
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer wrong key fails
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-wrong-key-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'hello' }], { realtimeUsec: 1_500_000n });
    writer.close();

    const wrongKey = '000000000000000000000001/1-f4240';
    verifyJournalFileWithKeyFailsIfAvailable(journalPath, wrongKey);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// Sealed writer tamper fails
{
  const tempDir = mkdtempSync(join(tmpdir(), 'node-sealed-tamper-'));
  try {
    const journalPath = join(tempDir, 'sealed.journal');
    const writer = Writer.create(journalPath, { seal: testSealOpts() });
    writer.append([{ name: 'MESSAGE', value: 'sealed-covered-stock' }], { realtimeUsec: 1_500_000n });
    writer.append([{ name: 'MESSAGE', value: 'later-entry' }], { realtimeUsec: 2_500_000n });
    writer.close();

    tamperDataPayload(journalPath, Buffer.from('MESSAGE=sealed-covered-stock'));

    const key = testVerificationKey(testSealOpts());
    verifyJournalFileWithKeyFailsIfAvailable(journalPath, key);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

const manifestPath = join(repoRoot, 'tests/conformance/manifests/conformance-v01.json');
if (!existsSync(manifestPath)) {
  throw new Error(`missing conformance manifest: ${manifestPath}`);
}

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const failures = [];
const results = [];
const expectedSkips = new Set();

for (const testCase of manifest.test_suite.test_cases) {
  const stdout = run(process.execPath, ['node/adapter/index.js', 'run'], {
    cwd: repoRoot,
    input: JSON.stringify(testCase),
  });
  const result = JSON.parse(stdout);
  results.push(result);
  if (result.status === 'FAIL' || result.status === 'ERROR') {
    failures.push(result);
  }
  if (result.status === 'SKIP' && !expectedSkips.has(result.test_name)) {
    failures.push({ ...result, status: 'FAIL', error: `unexpected SKIP: ${result.note || ''}` });
  }
}

assert.equal(results.length, manifest.test_suite.test_cases.length);

if (failures.length > 0) {
  for (const failure of failures) {
    process.stderr.write(`${failure.status}: ${failure.test_name}: ${failure.error || failure.note || ''}\n`);
  }
  throw new Error(`${failures.length} conformance case(s) failed`);
}

process.stdout.write(`PASS node package tests (${relative(repoRoot, manifestPath)})\n`);
