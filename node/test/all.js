#!/usr/bin/env node

import { closeSync, existsSync, mkdtempSync, openSync, readdirSync, readFileSync, rmSync, writeSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { jenkinsHash64, sipHash24 } from '../src/lib/hash.js';
import { Writer } from '../src/lib/writer.js';
import { Log } from '../src/lib/directory-writer.js';
import { FileReader } from '../src/lib/reader.js';
import { parseDataObject } from '../src/lib/entry.js';
import { exportEntry, jsonEntry, SdJournalOpen, SdJournalQueryUnique } from '../src/facade.js';
import {
  DATA_OBJECT_HEADER_SIZE,
  HEADER_SIZE,
  INCOMPATIBLE_COMPACT,
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_KEYED_HASH,
  OBJECT_COMPRESSED_LZ4,
  OBJECT_COMPRESSED_XZ,
  OBJECT_TYPE_DATA,
  STATE_ARCHIVED,
  parseObjectHeader,
  writeObjectHeader,
} from '../src/lib/header.js';
import { compressLz4DataPayload } from '../src/lib/lz4-block.js';
import { compressXzDataPayload, decompressXzDataPayload } from '../src/lib/xz-block.js';

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, '..');
const repoRoot = resolve(packageRoot, '..');

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
    const journalPath = join(tempDir, 'writer-lock.journal');
    const writer = Writer.create(journalPath);
    try {
      writer.append([{ name: 'MESSAGE', value: 'held' }]);
      assert.ok(existsSync(`${journalPath}.lock`));
      assert.throws(() => Writer.open(journalPath), /journal writer lock held/);
      assert.throws(() => Writer.create(journalPath), /journal writer lock held/);
    } finally {
      writer.close();
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
  // Verify no node-liblzma .node native addon is loaded at runtime.
  const req = createRequire(import.meta.url);
  const nativeAddonKeys = Object.keys(req.cache).filter(
    (k) => k.includes('node-liblzma') && k.endsWith('.node'),
  );
  assert.equal(nativeAddonKeys.length, 0, 'node-liblzma .node native addon must not be loaded');
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
    assert.deepEqual(facadeValues[0][1], Buffer.from([0xff]));
    assert.deepEqual(facadeValues[1][1], Buffer.from([0xef, 0xbf, 0xbd]));
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

assert.throws(() => new Log('/tmp', { source: '../bad' }), /invalid journal source/);

for (const file of listJavaScriptFiles(packageRoot).sort()) {
  run(process.execPath, ['--check', file], { cwd: repoRoot });
}

run(process.execPath, ['-e', "import './node/src/index.js'"], { cwd: repoRoot });

const manifestPath = join(repoRoot, 'tests/conformance/manifests/conformance-v01.json');
if (!existsSync(manifestPath)) {
  throw new Error(`missing conformance manifest: ${manifestPath}`);
}

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const failures = [];
const results = [];
const expectedSkips = new Set(['journal-verify-sealed', 'journal-verify-corruption-detection']);

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
