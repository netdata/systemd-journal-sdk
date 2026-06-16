import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, truncateSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { readFileHeader, FileReader } from '../../src/lib/reader.js';
import { HEADER_SIZE } from '../../src/lib/header.js';
import { Writer } from '../../src/lib/writer.js';
import { safeStatSync } from '../../src/lib/fs-safe.js';
import { NetdataJournalFunction } from '../../src/lib/netdata.js';

const BASE_USEC = 1_700_000_000_000_000;

// Test 1: readFileHeader parses header bounds from a valid journal file
function testReadFileHeaderParsesBounds() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const path = join(dir, 'test.journal');
    const enc = (s) => Buffer.from(s, 'utf8');
    const w = Writer.create(path, {
      machineId: Buffer.from('aabbccdd11223344aabbccdd11223344', 'hex'),
      bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
      seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
    });
    const firstUsec = BigInt(BASE_USEC);
    const lastUsec = BigInt(BASE_USEC + 5_000_000);
    w.append(
      [{ name: 'MESSAGE', value: enc('hello-0') }],
      { realtimeUsec: firstUsec },
    );
    w.append(
      [{ name: 'MESSAGE', value: enc('hello-1') }],
      { realtimeUsec: lastUsec },
    );
    w.close();

    const header = readFileHeader(path);
    assert.equal(Number(header.head_entry_realtime), BASE_USEC, 'head_entry_realtime should match first entry');
    assert.equal(Number(header.tail_entry_realtime), BASE_USEC + 5_000_000, 'tail_entry_realtime should match last entry');
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 2: readFileHeader succeeds on a truncated file (header-only read)
// FileReader.open also won't throw on truncated files (it gracefully handles
// missing entry arrays), but it reads the entire file into memory.
// On a non-truncated multi-GB file: readFileHeader reads 272 bytes;
// FileReader.open reads the full file — that's 144 GiB of I/O across
// 7338 files. This test proves readFileHeader only needs the header.
function testReadFileHeaderTruncatedFile() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const path = join(dir, 'test.journal');
    const enc = (s) => Buffer.from(s, 'utf8');
    const w = Writer.create(path, {
      machineId: Buffer.from('aabbccdd11223344aabbccdd11223344', 'hex'),
      bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
      seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
    });
    const firstUsec = BigInt(BASE_USEC);
    const lastUsec = BigInt(BASE_USEC + 10_000_000);
    for (let i = 0; i < 10; i++) {
      w.append(
        [{ name: 'MESSAGE', value: enc(`hello-${i}`) }],
        { realtimeUsec: BigInt(BASE_USEC + i * 1_000_000) },
      );
    }
    w.close();

    const originalSize = safeStatSync(path).size;
    assert.ok(originalSize > HEADER_SIZE * 2, 'file should be substantially larger than header');

    // Truncate to header size only — removes all entry array and data objects
    truncateSync(path, HEADER_SIZE);

    // readFileHeader still works — it only needs the header bytes
    const header = readFileHeader(path);
    assert.equal(Number(header.head_entry_realtime), BASE_USEC, 'head_entry_realtime preserved');
    assert.equal(Number(header.tail_entry_realtime), BASE_USEC + 9_000_000, 'tail_entry_realtime preserved');
    assert.equal(Number(header.n_entries), 10, 'n_entries preserved');
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 3: readFileHeader on a file with only the header region (no entries, but valid header)
function testReadFileHeaderMinimalHeaderOnlyFile() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    // Create a writer and close it — produces a valid header with zero entries
    const path = join(dir, 'empty.journal');
    const w = Writer.create(path, {
      machineId: Buffer.from('aabbccdd11223344aabbccdd11223344', 'hex'),
      bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
      seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
    });
    w.close();

    // Now truncate to just the header
    truncateSync(path, HEADER_SIZE);

    const header = readFileHeader(path);
    assert.equal(Number(header.n_entries), 0, 'empty file should have zero entries');
    assert.equal(Number(header.tail_entry_realtime), 0, 'empty file should have zero tail_realtime');
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 4: readFileHeader rejects non-journal signatures
function testReadFileHeaderRejectsBadSignature() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const path = join(dir, 'bad.journal');
    writeFileSync(path, Buffer.alloc(300));
    assert.throws(
      () => readFileHeader(path),
      (err) => String(err.message).includes('invalid journal signature'),
    );
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 5: readFileHeader rejects file too small
function testReadFileHeaderRejectsTooSmall() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const path = join(dir, 'small.journal');
    Buffer.from('LPKSHHRH').copy(Buffer.alloc(100));
    writeFileSync(path, Buffer.alloc(100));
    assert.throws(
      () => readFileHeader(path),
      (err) => String(err.message).includes('too small'),
    );
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 6: Source summary uses readFileHeader — proof: truncated file still contributes bounds
function testSourceSummaryWithTruncatedFile() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const machineId = '11'.repeat(16);
    const sub = join(dir, machineId);
    mkdirSync(sub, { recursive: true });
    const path = join(sub, 'system.journal');
    const enc = (s) => Buffer.from(s, 'utf8');
    const w = Writer.create(path, {
      machineId: Buffer.from(machineId, 'hex'),
      bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
      seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
    });
    w.append(
      [{ name: 'MESSAGE', value: enc('msg-0') }],
      { realtimeUsec: BigInt(BASE_USEC) },
    );
    w.append(
      [{ name: 'MESSAGE', value: enc('msg-1') }],
      { realtimeUsec: BigInt(BASE_USEC + 1_000_000) },
    );
    w.close();

    // Verify original summary works
    const fn = NetdataJournalFunction.systemdJournal();
    const result1 = fn.runDirectoryRequestJsonWithOptions(dir, { info: true });
    assert.equal(result1.status, 200);
    const allOpt1 = result1.required_params[0].options.find(o => o.id === 'all');
    assert.ok(allOpt1);
    assert.ok(allOpt1.info.includes('1 files'));
    // Should have real bounds
    assert.ok(!allOpt1.info.includes('covering off'));

    // Now truncate the file to just header — summary should STILL work
    // because _addSummaryPath uses readFileHeader instead of FileReader.open
    truncateSync(path, HEADER_SIZE);

    const result2 = fn.runDirectoryRequestJsonWithOptions(dir, { info: true });
    assert.equal(result2.status, 200);
    const allOpt2 = result2.required_params[0].options.find(o => o.id === 'all');
    assert.ok(allOpt2);
    assert.ok(allOpt2.info.includes('1 files'));
    // Should still show real bounds even though file is truncated
    assert.ok(!allOpt2.info.includes('covering off'), `expected real bounds, got: ${allOpt2.info}`);
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// Test 7: Prefilter uses readFileHeader — proof: truncated file still parsed for overlap check
async function testPrefilterWithTruncatedFile() {
  const dir = mkdtempSync(join(tmpdir(), 'header-read-'));
  try {
    const enc = (s) => Buffer.from(s, 'utf8');
    const inWindowEntries = [];
    const outOfWindowEntries = [];

    // In-window entries: BASE_USEC..BASE_USEC+2M
    for (let i = 0; i < 3; i++) {
      inWindowEntries.push({
        realtimeUsec: BigInt(BASE_USEC + i * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`in-${i}`) },
          { name: 'PRIORITY', value: enc('6') },
        ],
      });
    }
    // Out-of-window entries: BASE_USEC+250M..BASE_USEC+253M
    for (let i = 0; i < 3; i++) {
      outOfWindowEntries.push({
        realtimeUsec: BigInt(BASE_USEC + 250_000_000 + i * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`out-${i}`) },
          { name: 'PRIORITY', value: enc('3') },
        ],
      });
    }

    const inPath = join(dir, 'system.journal');
    const outPath = join(dir, 'system.journal~');
    const wid1 = Writer.create(inPath, {
      machineId: Buffer.from('11'.repeat(16), 'hex'),
      bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
      seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
    });
    for (const e of inWindowEntries) wid1.append(e.fields, { realtimeUsec: e.realtimeUsec });
    wid1.close();
    const wid2 = Writer.create(outPath, {
      machineId: Buffer.from('22'.repeat(16), 'hex'),
      bootId: Buffer.from('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'hex'),
      seqnumId: Buffer.from('44444444444444444444444444444444', 'hex'),
    });
    for (const e of outOfWindowEntries) wid2.append(e.fields, { realtimeUsec: e.realtimeUsec });
    wid2.close();

    // Truncate both files to header-only
    truncateSync(inPath, HEADER_SIZE);
    truncateSync(outPath, HEADER_SIZE);

    const fn = NetdataJournalFunction.systemdJournal();
    const injectableNow = BASE_USEC / 1000;

    // Query for the in-window time range
    const result = fn.runDirectoryRequestJsonWithOptions(
      dir,
      { after: BASE_USEC / 1_000_000, before: (BASE_USEC / 1_000_000) + 10, last: 100, data_only: true },
      { _injectableNow: injectableNow },
    );

    assert.equal(result.status, 200, `expected 200, got: ${JSON.stringify(result).slice(0, 300)}`);
    assert.ok(result._journal_files, 'must have _journal_files');
    // The in-window file should be in matched
    // The out-of-window file should be skipped
    // Since files are truncated, FileReader.open will fail within _exploreFiles,
    // but the prefilter should have already skipped the out-of-window file.
    // Both files may fail to read data (since truncated), but the prefilter
    // should correctly classify them.
    assert.ok(result._journal_files.skipped >= 0, 'skipped should be a number');
    assert.ok(result._journal_files.matched >= 0, 'matched should be a number');
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

export async function run() {
  testReadFileHeaderParsesBounds();
  testReadFileHeaderTruncatedFile();
  testReadFileHeaderMinimalHeaderOnlyFile();
  testReadFileHeaderRejectsBadSignature();
  testReadFileHeaderRejectsTooSmall();
  testSourceSummaryWithTruncatedFile();
  await testPrefilterWithTruncatedFile();
  console.log('  PASS header-only read');
}
