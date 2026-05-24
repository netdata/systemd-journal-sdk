#!/usr/bin/env node

import { closeSync, existsSync, mkdtempSync, openSync, readdirSync, readFileSync, rmSync, writeSync } from 'node:fs';
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
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_KEYED_HASH,
  OBJECT_COMPRESSED_LZ4,
  OBJECT_COMPRESSED_XZ,
  OBJECT_TYPE_DATA,
  STATE_ARCHIVED,
  writeObjectHeader,
} from '../src/lib/header.js';
import { compressLz4DataPayload } from '../src/lib/lz4-block.js';

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
    flags.writeUInt32LE(INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_XZ, 0);
    writeSync(fd, flags, 0, flags.length, 12);
    closeSync(fd);

    assert.throws(() => Writer.open(journalPath), /unsupported journal: incompatible flags/);
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
  writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_XZ, BigInt(DATA_OBJECT_HEADER_SIZE + 3));
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
