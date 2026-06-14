// Tests for SOW-0108 Node bounded reader access.

import { ftruncateSync } from 'node:fs';
import * as support from '../support.js';
import { CombinedResult } from '../../src/lib/netdata.js';
import {
  ExplorerFieldMode,
  ExplorerQuery,
  ExplorerStrategy,
} from '../../src/lib/explorer.js';

const {
  closeSync,
  mkdtempSync,
  rmSync,
  writeSync,
  tmpdir,
  join,
  zstdCompressSync,
  assert,
  Writer,
  FileReader,
  DirectoryReader,
  SdJournalOpenFiles,
  verifyFile,
  safeOpenSync,
  safeReadFileSync,
  safeWriteFileSync,
  repoRoot,
  READER_ACCESS_AUTO,
  READER_ACCESS_READ_AT,
  READER_ACCESS_MMAP,
  READER_BOUNDS_SNAPSHOT,
  UnsupportedAccessModeError,
} = support;

const READER_OPTIONS = Object.freeze({
  accessMode: READER_ACCESS_READ_AT,
  bounds: READER_BOUNDS_SNAPSHOT,
  windowSizeBytes: 128,
  maxWindows: 1,
  maxRowArenaBytes: 1024 * 1024,
});

function buildJournal(dir) {
  const path = join(dir, 'reader-access.journal');
  const writer = Writer.create(path, {
    machineId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
    bootId: Buffer.from('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'hex'),
    seqnumId: Buffer.from('cccccccccccccccccccccccccccccccc', 'hex'),
  });
  for (let i = 0; i < 12; i++) {
    writer.appendRaw(
      [
        Buffer.from(`MESSAGE=${'x'.repeat(80)}-${i}`),
        Buffer.from(`PRIORITY=${i % 8}`),
        Buffer.from(`SYSLOG_IDENTIFIER=sow0108-${i % 3}`),
        Buffer.from(`FIELD_A=${'a'.repeat(40)}-${i}`),
        Buffer.from(`FIELD_B=${'b'.repeat(40)}-${i}`),
      ],
      { realtimeUsec: 1_700_000_000_000_000n + BigInt(i) },
    );
  }
  writer.closeOffline();
  return path;
}

function buildSingleRowJournal(dir, payload, options = {}) {
  const path = join(dir, options.name ?? 'single-row.journal');
  const writer = Writer.create(path, {
    machineId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
    bootId: Buffer.from('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'hex'),
    seqnumId: Buffer.from('cccccccccccccccccccccccccccccccc', 'hex'),
    compression: options.compression,
    compressionThresholdBytes: options.compressionThresholdBytes,
  });
  writer.appendRaw(
    [
      Buffer.from(`MESSAGE=${payload}`),
      Buffer.from('PRIORITY=5'),
    ],
    { realtimeUsec: 1_700_000_000_000_000n },
  );
  writer.closeOffline();
  return path;
}

function appendOneEntry(path) {
  const writer = Writer.open(path);
  writer.appendRaw(
    [
      Buffer.from('MESSAGE=appended'),
      Buffer.from('PRIORITY=6'),
    ],
    { realtimeUsec: 1_700_000_000_001_000n },
  );
  writer.closeOffline();
}

function testExplicitMmapRejects() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-mmap-'));
  try {
    const path = buildJournal(dir);
    assert.throws(
      () => FileReader.open(path, { accessMode: READER_ACCESS_MMAP }),
      UnsupportedAccessModeError,
      'explicit mmap must fail clearly in Node core',
    );
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testExplicitReadAtSelectsReadAt() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-readat-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      accessMode: READER_ACCESS_READ_AT,
    });
    const stats = reader.accessStats();
    assert.equal(stats.requestedAccessMode, READER_ACCESS_READ_AT);
    assert.equal(stats.selectedAccessMode, READER_ACCESS_READ_AT);
    assert.equal(stats.selectedBackend, READER_ACCESS_READ_AT);
    assert.equal(stats.fallbackReason, '');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testAutoSelectsBoundedReadAt() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-auto-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      accessMode: READER_ACCESS_AUTO,
    });
    const stats = reader.accessStats();
    assert.equal(stats.requestedAccessMode, READER_ACCESS_AUTO);
    assert.equal(stats.selectedBackend, READER_ACCESS_READ_AT);
    assert.match(stats.fallbackReason, /no mmap API/i);
    assert.equal(stats.windowSizeBytes, READER_OPTIONS.windowSizeBytes);
    assert.equal(stats.maxWindows, READER_OPTIONS.maxWindows);
    assert.ok(stats.readSyncUsesPosition);
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testQueryUniqueUsesTemporaryDataReads() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-unique-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    const originalRowView = reader.accessor.rowView.bind(reader.accessor);
    reader.accessor.rowView = () => {
      throw new Error('queryUnique used row-lifetime rowView');
    };
    const values = reader.queryUnique('SYSLOG_IDENTIFIER');
    reader.accessor.rowView = originalRowView;
    assert.ok(values.length >= 3, 'unique query should return field values');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testRowLifetimeSurvivesWindowEvictionPressure() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-row-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();

    const firstPayload = reader.enumerateEntryPayload();
    assert.ok(firstPayload, 'first payload must exist');
    const expected = Buffer.from(firstPayload);

    for (;;) {
      const payload = reader.enumerateEntryPayload();
      if (payload === null) break;
      assert.ok(payload.includes(0x3d), 'payload must be FIELD=VALUE');
    }

    assert.deepEqual(Buffer.from(firstPayload), expected,
      'current-row payload view must remain valid through the full row');
    const stats = reader.accessStats();
    assert.ok(stats.windowsCreated > 0, 'reader must use bounded windows');
    assert.ok(stats.readBufferBytes <= READER_OPTIONS.windowSizeBytes * READER_OPTIONS.maxWindows,
      `window memory exceeds budget: ${stats.readBufferBytes}`);
    assert.ok(stats.rowArenaPeakBytes <= READER_OPTIONS.maxRowArenaBytes,
      `row arena exceeds budget: ${stats.rowArenaPeakBytes}`);
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testEntryDataRestartPreservesCurrentRowViews() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-restart-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();
    const firstPayload = reader.enumerateEntryPayload();
    const expected = Buffer.from(firstPayload);

    reader.entryDataRestart();
    while (reader.enumerateEntryPayload() !== null) {
      // Exhausting current-row DATA must not invalidate prior same-row views.
    }

    assert.deepEqual(Buffer.from(firstPayload), expected,
      'entryDataRestart and end-of-enumeration preserve same-row payload views');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testOversizedPayloadUsesRowArenaAndLimitIsEnforced() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-oversized-'));
  let reader = null;
  try {
    const path = buildSingleRowJournal(dir, 'm'.repeat(512));
    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      windowSizeBytes: 64,
      maxWindows: 1,
      maxRowArenaBytes: 4096,
    });
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();
    const payload = reader.enumerateEntryPayload();
    assert.ok(payload.length > 64, 'test payload must exceed the configured window size');
    let stats = reader.accessStats();
    assert.ok(stats.rowArenaPeakBytes >= payload.length,
      'oversized row payload should be backed by row arena memory');
    assert.ok(stats.readBufferBytes <= 64, 'window memory must stay bounded');
    reader.close();
    reader = null;

    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      windowSizeBytes: 64,
      maxWindows: 1,
      maxRowArenaBytes: 32,
    });
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();
    assert.throws(() => reader.enumerateEntryPayload(), /row arena limit exceeded/);
    stats = reader.accessStats();
    assert.ok(stats.readBufferBytes <= 64, 'failed row arena read must not grow window memory');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testCompressedDataUsesRowArena() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-compressed-'));
  let reader = null;
  try {
    const path = buildSingleRowJournal(dir, 'z'.repeat(4096), {
      name: 'compressed.journal',
      compression: 'zstd',
      compressionThresholdBytes: 8,
    });
    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      windowSizeBytes: 1024,
      maxWindows: 2,
      maxRowArenaBytes: 16 * 1024,
    });
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();
    const payload = reader.enumerateEntryPayload();
    assert.ok(payload.includes(0x3d), 'compressed payload should decompress to FIELD=VALUE');
    const stats = reader.accessStats();
    assert.ok(stats.rowArenaPeakBytes >= payload.length,
      'compressed DATA should be decompressed into row arena memory');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testOwnedEntryDataSurvivesNextRow() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-owned-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    reader.seekHead();
    assert.equal(reader.next(), true);
    const entry = reader.getEntry();
    const message = Buffer.from(entry.fields.MESSAGE);

    assert.equal(reader.next(), true);
    assert.deepEqual(Buffer.from(entry.fields.MESSAGE), message,
      'getEntry() returns owned entry buffers that survive row advance');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testSnapshotBoundsIgnoreAppendedEntries() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-snapshot-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    let count = 0;
    reader.seekHead();
    while (reader.next()) count++;
    assert.equal(count, 12, 'baseline fixture row count');

    appendOneEntry(path);
    assert.equal(reader.refresh(), false, 'snapshot reader must not refresh appended data');
    count = 0;
    reader.seekHead();
    while (reader.next()) count++;
    assert.equal(count, 12, 'snapshot reader must keep open-time row set');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testRefreshFailureRestoresVisibleBoundsAndRowViews() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-refresh-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, {
      ...READER_OPTIONS,
      bounds: support.READER_BOUNDS_LIVE,
    });
    reader.seekHead();
    assert.equal(reader.next(), true);
    reader.entryDataRestart();
    const payload = reader.enumerateEntryPayload();
    const expected = Buffer.from(payload);
    const oldBounds = reader.accessor.snapshotVisibleBounds();

    reader.accessor.refreshVisibleBounds = () => {
      reader.accessor.visibleSize = oldBounds + 8;
      return true;
    };
    reader._readEntryArrayOffsets = () => {
      throw new Error('forced entry-array reload failure');
    };

    assert.equal(reader.refresh(), false, 'failed refresh must roll back');
    assert.equal(reader.accessor.snapshotVisibleBounds(), oldBounds,
      'refresh failure must restore pre-refresh accessor bounds');
    assert.deepEqual(Buffer.from(payload), expected,
      'refresh failure must not invalidate current-row payload views');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testShortReadsAndInvalidOffsetsFailPredictably() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-short-'));
  let reader = null;
  let fd;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    const size = reader.accessor.size();
    assert.throws(() => reader._readBytes(size + 1, 1), /visible file bounds/);

    fd = safeOpenSync(path, 'r+');
    ftruncateSync(fd, size - 2);
    closeSync(fd);
    fd = undefined;
    assert.throws(() => reader._readBytes(size - 4, 4), /short read before visible file size/);
  } finally {
    if (fd !== undefined) {
      try { closeSync(fd); } catch {}
    }
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testCorruptedEntryArrayCycleFails() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-cycle-'));
  let reader = null;
  let fd;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    const entryArrayOffset = reader.header.entry_array_offset;
    reader.close();
    reader = null;

    const buf = Buffer.alloc(8);
    buf.writeBigUInt64LE(entryArrayOffset);
    fd = safeOpenSync(path, 'r+');
    writeSync(fd, buf, 0, buf.length, Number(entryArrayOffset) + 16);
    closeSync(fd);
    fd = undefined;

    assert.throws(() => FileReader.open(path, READER_OPTIONS), /entry array chain cycle/);
  } finally {
    if (fd !== undefined) {
      try { closeSync(fd); } catch {}
    }
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testDirectoryAndFacadePropagateReaderOptions() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-dir-'));
  let directory = null;
  let journal = null;
  try {
    const path = buildJournal(dir);
    directory = DirectoryReader.openFiles([path], READER_OPTIONS);
    assert.equal(directory.readers[0].accessStats().windowSizeBytes, READER_OPTIONS.windowSizeBytes);
    directory.close();
    directory = null;

    journal = SdJournalOpenFiles([path], 0, READER_OPTIONS);
    assert.equal(journal.reader.accessStats().windowSizeBytes, READER_OPTIONS.windowSizeBytes);
  } finally {
    try { directory?.close(); } catch {}
    try { journal?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testZstFilesUseBoundedAccessor() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-zst-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    const zstPath = `${path}.zst`;
    safeWriteFileSync(zstPath, zstdCompressSync(safeReadFileSync(path)));

    reader = FileReader.open(zstPath, READER_OPTIONS);
    const stats = reader.accessStats();
    assert.equal(stats.bounds, READER_BOUNDS_SNAPSHOT,
      '.journal.zst temporary journal must be read as a snapshot');
    reader.seekHead();
    assert.equal(reader.next(), true);
    assert.ok(reader.getEntry().fields.MESSAGE);
    reader.close();
    reader = null;

    verifyFile(zstPath, READER_OPTIONS);
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testProductionReaderSurfacesDoNotBypassAccessor() {
  const strictReaderForbidden = [
    /\bsafeReadFileSync\b/,
    /\breadFileSync\b/,
    /\bdecompressZstToTemp\b/,
    /\bthis\.buffer\b/,
    /\breader\.buffer\b/,
    /\br\.buffer\b/,
  ];
  const readerBufferForbidden = [
    /\bdecompressZstToTemp\b/,
    /\bthis\.buffer\b/,
    /\breader\.buffer\b/,
    /\br\.buffer\b/,
  ];
  const files = [
    ['node/src/lib/reader.js', strictReaderForbidden],
    ['node/src/lib/directory-reader.js', strictReaderForbidden],
    ['node/src/facade.js', strictReaderForbidden],
    ['node/src/lib/explorer.js', strictReaderForbidden],
    ['node/src/lib/netdata.js', strictReaderForbidden],
    ['node/src/lib/verify.js', strictReaderForbidden],
    ['node/src/lib/verify-graph.js', strictReaderForbidden],
    ['node/cmd/journalctl/index.js', strictReaderForbidden],
    ['node/cmd/reader_core_bench.js', strictReaderForbidden],
    ['node/adapter/index.js', readerBufferForbidden],
    ['node/src/lib/compress.js', [/\bdecompressZstToTemp\b/]],
  ];
  for (const [rel, forbidden] of files) {
    const text = safeReadFileSync(join(repoRoot, rel), 'utf8');
    for (const pattern of forbidden) {
      assert.equal(pattern.test(text), false, `${rel} bypasses reader accessor with ${pattern}`);
    }
  }
}

function testCombinedResultCarriesReaderOptionsForBackfill() {
  const readerOptions = Object.freeze({
    accessMode: READER_ACCESS_READ_AT,
    bounds: READER_BOUNDS_SNAPSHOT,
    windowSizeBytes: 4096,
    maxWindows: 2,
  });
  const result = new CombinedResult({ readerOptions });
  assert.equal(result.readerOptions, readerOptions,
    'Netdata zero-count facet backfill must keep caller reader options');
}

function testExplorerIndexedFieldChainsUseTemporaryDataReads() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0108-indexed-'));
  let reader = null;
  try {
    const path = buildJournal(dir);
    reader = FileReader.open(path, READER_OPTIONS);
    const originalReadDataPayloadAt = reader._readDataPayloadAt.bind(reader);
    reader._readDataPayloadAt = (offset, rowLifetime = true) => {
      assert.equal(rowLifetime, false,
        `indexed FIELD-chain read at ${offset} must use temporary DATA reads`);
      return originalReadDataPayloadAt(offset, rowLifetime);
    };

    const query = new ExplorerQuery().withFacet('PRIORITY').withHistogram('PRIORITY');
    query.useSourceRealtime = false;
    query.fieldMode = ExplorerFieldMode.AllValues;
    query.limit = 0;
    const result = reader.exploreWithStrategy(query, ExplorerStrategy.Index);
    assert.ok(result.facets.size > 0, 'indexed facet collection should run');
    assert.ok(result.histogram !== null, 'indexed histogram collection should run');
  } finally {
    try { reader?.close(); } catch {}
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

export async function run() {
  testExplicitMmapRejects();
  testExplicitReadAtSelectsReadAt();
  testAutoSelectsBoundedReadAt();
  testQueryUniqueUsesTemporaryDataReads();
  testRowLifetimeSurvivesWindowEvictionPressure();
  testEntryDataRestartPreservesCurrentRowViews();
  testOversizedPayloadUsesRowArenaAndLimitIsEnforced();
  testCompressedDataUsesRowArena();
  testOwnedEntryDataSurvivesNextRow();
  testSnapshotBoundsIgnoreAppendedEntries();
  testRefreshFailureRestoresVisibleBoundsAndRowViews();
  testShortReadsAndInvalidOffsetsFailPredictably();
  testCorruptedEntryArrayCycleFails();
  testDirectoryAndFacadePropagateReaderOptions();
  testZstFilesUseBoundedAccessor();
  testProductionReaderSurfacesDoNotBypassAccessor();
  testCombinedResultCarriesReaderOptionsForBackfill();
  testExplorerIndexedFieldChainsUseTemporaryDataReads();
}
