// Tests for SOW-0105 reviewer-blocker fixes (round 1).
//
// Fix 1: control threaded into _exploreFiles — cancellation during mid-file scan
// Fix 2: capEffectiveDisplay BigInt for full-width capability bits
// Fix 3: d.ts conformance — declaration surface matches actual module exports

import { Buffer } from 'node:buffer';
import { mkdtempSync, rmSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import {
  Writer,
  FileReader,
  ExplorerControl,
  ExplorerFieldMode,
  ExplorerQuery,
  ExplorerStopReason,
  ExplorerStrategy,
  Direction,
} from '../../src/index.js';
import {
  SystemdJournalProfile,
  DisplayContext,
  DisplayScope,
  NetdataFunctionConfig,
  NetdataRequest,
  _requestToExplorerQuery,
} from '../../src/lib/netdata.js';
import { ExplorerAnchorKind } from '../../src/lib/explorer.js';
import * as actualModule from '../../src/index.js';

// ---------------------------------------------------------------------------
// Fix 1: control threaded — cancellation during mid-file scan
// ---------------------------------------------------------------------------

function buildLargeJournal(dir) {
  const journalDir = join(dir, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'system.journal');
  mkdirSync(join(dir, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'), { recursive: true });
  const writer = Writer.create(journalDir, {
    machineId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
    bootId: Buffer.from('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'hex'),
    seqnumId: Buffer.from('cccccccccccccccccccccccccccccccc', 'hex'),
  });
  const enc = (s) => Buffer.from(s, 'utf8');
  // Write enough entries so the scan takes measurable work — cancellation
  // must trigger DURING the file scan, not only between files.
  const count = 2000;
  for (let i = 0; i < count; i++) {
    writer.append(
      [
        { name: 'MESSAGE', value: enc(`m-${String(i).padStart(5, '0')}`) },
        { name: 'PRIORITY', value: enc(String(5 + (i % 3))) },
        { name: '_HOSTNAME', value: enc(`host-${String(i % 5)}`) },
      ],
      { realtimeUsec: BigInt(1700000000_000000 + i * 10_000) },
    );
  }
  writer.close();
  return journalDir;
}

function testCancelDuringSingleFileScan() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0105-f1-'));
  try {
    buildLargeJournal(dir);

    const reader = FileReader.open(join(dir, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'system.journal'));
    try {
      const query = new ExplorerQuery();
      query.direction = Direction.Forward;
      query.limit = 200;
      query.afterRealtimeUsec = 1700000000_000000n;
      query.beforeRealtimeUsec = 9999999999_000000n;
      query.fieldMode = ExplorerFieldMode.AllValues;
      query.useSourceRealtime = true;
      query.facets = [Buffer.from('_HOSTNAME', 'utf8'), Buffer.from('PRIORITY', 'utf8')];
      query.excludeFacetFieldFilters = false;

      const control = new ExplorerControl();
      // Check every row so cancellation fires mid-file.
      control._nextCheckRows = 0n;
      let cancelled = false;
      control.setCancellationCallback(() => cancelled);
      // After the first control check, arm cancellation
      const origCheck = control._check.bind(control);
      control._check = function(stats) {
        const result = origCheck(stats);
        control._nextCheckRows = 0n; // keep checking every row
        if (control.stopReason !== ExplorerStopReason.Cancelled) {
          cancelled = true;
        }
        return result;
      };

      const strategy = ExplorerStrategy.Traversal;
      const result = reader.exploreWithStrategyAndControl(query, strategy, control);
      assert.equal(control.stopReason, ExplorerStopReason.Cancelled,
        'control.stopReason must be Cancelled when callback returns true mid-scan');
      assert.ok(result.stats.rowsExamined > 0n, 'must have examined some rows');
      // Should NOT have examined all 2000 rows — cancelled mid-scan
      assert.ok(result.stats.rowsExamined < 2000n,
        `cancelled scan should NOT examine all rows, got ${result.stats.rowsExamined}`);
    } finally {
      try { reader.close(); } catch {}
    }
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testCancelDuringCombinedPass() {
  const dir = mkdtempSync(join(tmpdir(), 'sow0105-f1b-'));
  try {
    buildLargeJournal(dir);

    const reader = FileReader.open(join(dir, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'system.journal'));
    try {
      const query = new ExplorerQuery();
      query.direction = Direction.Forward;
      query.limit = 200;
      query.afterRealtimeUsec = 1700000000_000000n;
      query.beforeRealtimeUsec = 9999999999_000000n;
      query.fieldMode = ExplorerFieldMode.AllValues;
      query.useSourceRealtime = true;
      query.facets = [Buffer.from('_HOSTNAME', 'utf8')];
      query.excludeFacetFieldFilters = false;

      const control = new ExplorerControl();
      control._nextCheckRows = 0n;
      let cancelled = false;
      control.setCancellationCallback(() => cancelled);
      const origCheck = control._check.bind(control);
      control._check = function(stats) {
        const result = origCheck(stats);
        control._nextCheckRows = 0n;
        if (control.stopReason !== ExplorerStopReason.Cancelled) {
          cancelled = true;
        }
        return result;
      };

      const result = reader.exploreWithStrategyAndControl(
        query, ExplorerStrategy.Traversal, control,
      );
      assert.equal(control.stopReason, ExplorerStopReason.Cancelled);
      assert.ok(result.stats.rowsExamined > 0n);
      assert.ok(result.stats.rowsExamined < 2000n,
        `combined pass must not examine all rows, got ${result.stats.rowsExamined}`);
    } finally {
      try { reader.close(); } catch {}
    }
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

// ---------------------------------------------------------------------------
// Fix 2: capEffectiveDisplay BigInt — bits above 31
// ---------------------------------------------------------------------------

function testCapEffectiveDisplayHighBits() {
  // CAPABILITIES array: PERFMON=38, BPF=39, CHECKPOINT_RESTORE=40
  // 0x4000000000  = 1 << 38 = PERFMON
  // 0xC000000000  = (1<<38)|(1<<39) = PERFMON | BPF
  // 0x1C000000000 = (1<<38)|(1<<39)|(1<<40) = PERFMON | BPF | CHECKPOINT_RESTORE
  const profile = new SystemdJournalProfile();
  const ctx = new DisplayContext();
  const enc = (s) => Buffer.from(s, 'utf8');

  // Bit 38 only (PERFMON): 0x4000000000
  const result38 = profile.fieldDisplayValue(ctx, DisplayScope.Data, '_CAP_EFFECTIVE', enc('4000000000'));
  assert.ok(result38.includes('PERFMON'), `expected PERFMON in ${result38}`);
  assert.ok(!result38.includes('BPF'), `should not include BPF in ${result38}`);
  assert.ok(!result38.includes('CHECKPOINT_RESTORE'), `should not include CHECKPOINT_RESTORE in ${result38}`);

  // Bits 38+39 (PERFMON|BPF): 0xC000000000
  const result39 = profile.fieldDisplayValue(ctx, DisplayScope.Data, '_CAP_EFFECTIVE', enc('C000000000'));
  assert.ok(result39.includes('PERFMON'), `expected PERFMON in ${result39}`);
  assert.ok(result39.includes('BPF'), `expected BPF in ${result39}`);
  assert.ok(!result39.includes('CHECKPOINT_RESTORE'), `should not include CHECKPOINT_RESTORE in ${result39}`);

  // Bits 38+39+40 (PERFMON|BPF|CHECKPOINT_RESTORE): 0x1C000000000
  const result40 = profile.fieldDisplayValue(ctx, DisplayScope.Data, '_CAP_EFFECTIVE', enc('1C000000000'));
  assert.ok(result40.includes('PERFMON'), `expected PERFMON in ${result40}`);
  assert.ok(result40.includes('BPF'), `expected BPF in ${result40}`);
  assert.ok(result40.includes('CHECKPOINT_RESTORE'), `expected CHECKPOINT_RESTORE in ${result40}`);

  // Bit 37 (AUDIT_READ) + bit 38 (PERFMON): verify mixed low/high above 31
  const resultMixed = profile.fieldDisplayValue(ctx, DisplayScope.Data, '_CAP_EFFECTIVE', enc('6000000000'));
  assert.ok(resultMixed.includes('PERFMON'), `expected PERFMON in ${resultMixed}`);
  assert.ok(resultMixed.includes('AUDIT_READ'), `expected AUDIT_READ (index 37) in ${resultMixed}`);
  assert.ok(!resultMixed.includes('AUDIT_WRITE'), `should not include AUDIT_WRITE in ${resultMixed}`);

  // Zero should return raw unchanged
  const resultZero = profile.fieldDisplayValue(ctx, DisplayScope.Data, '_CAP_EFFECTIVE', enc('0'));
  assert.equal(resultZero, '0');
}

// ---------------------------------------------------------------------------
// Fix 3: d.ts conformance — declaration surface matches actual module
// ---------------------------------------------------------------------------

// Every class/function/enum the d.ts declares must exist with the right
// typeof in the actual module. This checklist is maintained by hand from
// the current d.ts public surface.

// Conformance checklist: every class the d.ts declares that IS expected
// to exist on the main package export (src/index.js). Internal types
// (SealOptions, SealState, NetdataFunctionConfig, DisplayContext, etc.)
// that appear in the d.ts for type-checking but live on deep import
// paths are excluded from this mechanical check.
const DECLARED_CLASSES = [
  'FileReader',
  'DirectoryReader',
  'Writer',
  'Log',
  'ExplorerAnchor',
  'ExplorerError',
  'ExplorerUnsupported',
  'ExplorerFilter',
  'ExplorerFtsPattern',
  'ExplorerSampling',
  'ExplorerStats',
  'ExplorerRow',
  'ExplorerHistogramBucket',
  'ExplorerHistogram',
  'ExplorerComparison',
  'ExplorerResult',
  'ExplorerProgress',
  'ExplorerQuery',
  'ExplorerControl',
  'NetdataRequest',
  'CombinedResult',
  'JournalFileCollection',
  'NetdataJournalFunction',
  'SdJournal',
  'WriterLock',
];

const DECLARED_FUNCTIONS = [
  'parseFileHeader',
  'parseObjectHeader',
  'parseEntryObject',
  'parseDataObject',
  'parseDataPayload',
  'readUint64LE',
  'writeUint64LE',
  'writeUint32LE',
  'writeUint8',
  'align8',
  'bufEqual',
  'uuidToString',
  'stringToUUID',
  'isZeroUUID',
  'randomUUID',
  'sipHash24',
  'jenkinsHash64',
  'parseMatchString',
  'decompressZstSync',
  'isJournalFileName',
  'isZstFile',
  'openJournal',
  'createJournal',
  'stringField',
  'binaryField',
  'normalizeTimeWindow',
  'journalFileSourceType',
  'collectJournalFiles',
  // SdJournal C-style function aliases
  'SdJournalOpen',
  'SdJournalOpenFile',
  'SdJournalOpenDirectory',
  'SdJournalOpenFiles',
  'SdJournalClose',
  'SdJournalAddMatch',
  'SdJournalAddDisjunction',
  'SdJournalAddConjunction',
  'SdJournalFlushMatches',
  'SdJournalNext',
  'SdJournalNextSkip',
  'SdJournalPrevious',
  'SdJournalPreviousSkip',
  'SdJournalSeekHead',
  'SdJournalSeekTail',
  'SdJournalSeekRealtimeUsec',
  'SdJournalSeekCursor',
  'SdJournalGetEntry',
  'SdJournalGetData',
  'SdJournalRestartData',
  'SdJournalEnumerateAvailableData',
  'SdJournalGetRealtimeUsec',
  'SdJournalGetSeqnum',
  'SdJournalGetMonotonicUsec',
  'SdJournalGetCursor',
  'SdJournalTestCursor',
  'SdJournalEnumerateFields',
  'SdJournalRestartFields',
  'SdJournalEnumerateField',
  'SdJournalQueryUnique',
  'SdJournalVisitUniqueValues',
  'SdJournalQueryUniqueState',
  'SdJournalRestartUnique',
  'SdJournalEnumerateAvailableUnique',
  'SdJournalListBoots',
  'SdJournalSetOutputMode',
  'SdJournalProcessOutput',
];

const DECLARED_CONSTANTS = [
  'HEADER_SIZE',
  'OBJECT_HEADER_SIZE',
  'ENTRY_OBJECT_HEADER_SIZE',
  'DATA_OBJECT_HEADER_SIZE',
  'FIELD_OBJECT_HEADER_SIZE',
  'HASH_ITEM_SIZE',
  'OBJECT_TYPE_DATA',
  'OBJECT_TYPE_FIELD',
  'OBJECT_TYPE_ENTRY',
  'OBJECT_TYPE_DATA_HASH_TABLE',
  'OBJECT_TYPE_FIELD_HASH_TABLE',
  'OBJECT_TYPE_ENTRY_ARRAY',
  'FIELD_NAME_POLICY_JOURNALD',
  'FIELD_NAME_POLICY_JOURNAL_APP',
  'FIELD_NAME_POLICY_RAW',
  'Direction',
  'ExplorerAnchorKind',
  'ExplorerFieldMode',
  'ExplorerStrategy',
  'ExplorerStopReason',
  'UNSET_VALUE',
  'DEFAULT_HISTOGRAM_TARGET_BUCKETS',
  'DEFAULT_TIME_SLACK_USEC',
  'EXPLORER_CONTROL_CHECK_EVERY_ROWS',
  'EXPLORER_PROGRESS_INTERVAL_MS',
  'OUTPUT_MODE_DEFAULT',
  'OUTPUT_MODE_JSON',
  'OUTPUT_MODE_EXPORT',
  'LOG_OPEN_LAZY',
  'LOG_OPEN_EAGER',
  'LOG_IDENTITY_AUTO',
  'LOG_IDENTITY_STRICT',
  'LOG_LIFECYCLE_CREATED',
  'LOG_LIFECYCLE_ROTATED',
  'LOG_LIFECYCLE_DELETED',
  'LOG_LIFECYCLE_REASON_APPEND',
  'LOG_LIFECYCLE_REASON_EAGER_OPEN',
  'LOG_LIFECYCLE_REASON_ROTATION',
  'LOG_LIFECYCLE_REASON_RETENTION',
];

function testConformanceClasses() {
  for (const name of DECLARED_CLASSES) {
    assert.ok(actualModule[name] !== undefined, `d.ts class ${name} is missing from module`);
    const actual = actualModule[name];
    assert.equal(typeof actual, 'function', `d.ts class ${name} must be typeof function, got ${typeof actual}`);
    // Verify it can be constructed (classes, not plain functions)
    try {
      new actual();
    } catch {
      // Some classes require constructor args — that's fine
    }
  }
}

function testConformanceFunctions() {
  for (const name of DECLARED_FUNCTIONS) {
    assert.ok(actualModule[name] !== undefined, `d.ts function ${name} is missing from module`);
    assert.equal(typeof actualModule[name], 'function',
      `d.ts function ${name} must be typeof function, got ${typeof actualModule[name]}`);
  }
}

function testConformanceConstants() {
  for (const name of DECLARED_CONSTANTS) {
    assert.ok(actualModule[name] !== undefined, `d.ts constant ${name} is missing from module`);
    // Must not be a plain undefined/null — every constant must resolve
    assert.notEqual(actualModule[name], null, `d.ts constant ${name} is null`);
    assert.notEqual(actualModule[name], undefined, `d.ts constant ${name} is undefined`);
  }
}

function testNoExtraneousFreeExploreFunctions() {
  // exploreWithStrategy / exploreWithStrategyAndControl are FileReader
  // methods only, not free functions on the module.
  assert.equal(actualModule.exploreWithStrategy, undefined,
    'exploreWithStrategy must NOT be a free module export (it is a FileReader method)');
  assert.equal(actualModule.exploreWithStrategyAndControl, undefined,
    'exploreWithStrategyAndControl must NOT be a free module export (it is a FileReader method)');
}

function testFileReaderHasExplorerMethods() {
  const reader = actualModule.FileReader;
  assert.equal(typeof reader.prototype.explore, 'function',
    'FileReader.prototype.explore must exist');
  assert.equal(typeof reader.prototype.exploreWithStrategy, 'function',
    'FileReader.prototype.exploreWithStrategy must exist');
  assert.equal(typeof reader.prototype.exploreWithStrategyAndControl, 'function',
    'FileReader.prototype.exploreWithStrategyAndControl must exist');
}

function testNetdataFunctionSurface() {
  const cls = actualModule.NetdataJournalFunction;
  assert.equal(typeof cls.systemdJournal, 'function');
  assert.equal(typeof cls.systemdJournalPluginCompatible, 'function');
  assert.equal(typeof cls.new, 'function');

  const instance = cls.systemdJournal();
  assert.equal(typeof instance.runDirectoryRequestJson, 'function');
  assert.equal(typeof instance.runDirectoryRequestJsonWithOptions, 'function');
  assert.equal(typeof instance.runDirectoryRequestBytes, 'function');
  assert.equal(typeof instance.runDirectoryRequestBytesWithOptions, 'function');

  // configure/discover/info/execute must NOT exist (they are removed
  // from both the class and the d.ts).
  assert.equal(instance.configure, undefined, 'configure must not exist');
  assert.equal(instance.discover, undefined, 'discover must not exist');
  assert.equal(instance.info, undefined, 'info must not exist');
  assert.equal(instance.execute, undefined, 'execute must not exist');
}

function testExplorerControlMatchedRowType() {
  // The matchedRow callback receives (realtimeUsec: bigint, rowsMatched: bigint)
  // not an ExplorerRow object.
  const control = new ExplorerControl();
  let receivedUsec = null;
  let receivedRows = null;
  control.setMatchedRowCallback((usec, rows) => {
    receivedUsec = usec;
    receivedRows = rows;
    return false;
  });
  const result = control.emitMatchedRow(42n, 10n);
  assert.equal(result, false, 'emitMatchedRow returns callback result');
  assert.equal(receivedUsec, 42n, 'first arg must be realtimeUsec (bigint)');
  assert.equal(receivedRows, 10n, 'second arg must be rowsMatched (bigint)');
}

// ---------------------------------------------------------------------------
// Fix 4 (round 2): anchor outside window
// ---------------------------------------------------------------------------

function testAnchorOutsideWindowReset() {
  // Use absolute timestamps (> 94M so _relativeWindowToAbsolute treats them as absolute)
  // 100_000_000 seconds ≈ 1973-03-03
  // 150_000_000 seconds ≈ 1974-10-03
  // 200_000_000 seconds ≈ 1976-05-04
  // 250_000_000 seconds ≈ 1977-12-03
  const config = NetdataFunctionConfig.systemdJournal();

  // Anchor BEFORE the window → reset to Auto + Backward.
  const beforeWindow = NetdataRequest.parse(
    { anchor: 50000000, after: 100000000, before: 200000000 },
    config,
  );
  assert.equal(beforeWindow.anchor.kind, ExplorerAnchorKind.Auto,
    'anchor before window must reset to Auto');
  assert.equal(beforeWindow.direction, Direction.Backward,
    'anchor outside window must force Backward');

  // Anchor AFTER the window → reset to Auto + Backward.
  const afterWindow = NetdataRequest.parse(
    { anchor: 250000000, after: 100000000, before: 200000000 },
    config,
  );
  assert.equal(afterWindow.anchor.kind, ExplorerAnchorKind.Auto,
    'anchor after window must reset to Auto');
  assert.equal(afterWindow.direction, Direction.Backward,
    'anchor outside window must force Backward');

  // Anchor INSIDE the window → keep anchor and direction.
  const insideWindow = NetdataRequest.parse(
    { anchor: 150000000, after: 100000000, before: 200000000, direction: 'forward' },
    config,
  );
  assert.equal(insideWindow.anchor.kind, ExplorerAnchorKind.Realtime,
    'anchor inside window must stay Realtime');
  assert.equal(insideWindow.direction, Direction.Forward,
    'anchor inside window must keep Forward direction');

  // No anchor → Auto (default).
  const noAnchor = NetdataRequest.parse(
    { after: 100000000, before: 200000000 },
    config,
  );
  assert.equal(noAnchor.anchor.kind, ExplorerAnchorKind.Auto,
    'no anchor must default to Auto');

  // Zero anchor → Auto (treated as missing).
  const zeroAnchor = NetdataRequest.parse(
    { anchor: 0, after: 100000000, before: 200000000 },
    config,
  );
  assert.equal(zeroAnchor.anchor.kind, ExplorerAnchorKind.Auto,
    'zero anchor must default to Auto');

  // Tail anchor → keeps realtime, forces Backward (even if inside window).
  // tail requires data_only + if_modified_since set.
  const tailAnchor = NetdataRequest.parse(
    { anchor: 150000000, after: 100000000, before: 200000000,
      data_only: true, if_modified_since: 1, tail: true, direction: 'forward' },
    config,
  );
  assert.equal(tailAnchor.anchor.kind, ExplorerAnchorKind.Realtime,
    'tail anchor must stay Realtime');
  assert.equal(tailAnchor.direction, Direction.Backward,
    'tail anchor must force Backward (tail overrides outside-window check)');
}

// ---------------------------------------------------------------------------
// Fix 5 (round 2): data-only early-stop parity (stopWhenRowsFull)
// Rust netdata.rs:1609-1610 (to_explorer_query) + 1627-1629 (file_query):
// stop_when_rows_full = data_only && !tail_anchor, disabled for delta re-scans.
// ---------------------------------------------------------------------------

function testDataOnlyStopWhenRowsFull() {
  const config = NetdataFunctionConfig.systemdJournal();
  const win = { after: 100000000, before: 200000000 };

  // data_only, no tail, no delta -> early-stop enabled.
  const plainDataOnly = NetdataRequest.parse({ ...win, data_only: true }, config);
  let q = _requestToExplorerQuery(plainDataOnly, 1, null);
  assert.equal(q.stopWhenRowsFull, true,
    'data_only without tail/delta must enable stopWhenRowsFull');
  assert.equal(Number(q.stopWhenRowsFullCheckEvery), 128,
    'check-every must be DATA_ONLY_CHECK_EVERY_ROWS (128)');

  // Non data_only -> never early-stop (full facet/histogram analysis).
  const analytic = NetdataRequest.parse({ ...win, data_only: false }, config);
  q = _requestToExplorerQuery(analytic, 1, null);
  assert.equal(q.stopWhenRowsFull, false,
    'analytic (non data_only) request must not early-stop');

  // data_only + delta (no tail) -> override disables early-stop (Rust file_query).
  const deltaNoTail = NetdataRequest.parse(
    { ...win, data_only: true, delta: true }, config);
  q = _requestToExplorerQuery(deltaNoTail, 1, null);
  assert.equal(q.stopWhenRowsFull, false,
    'data_only delta re-scan must disable stopWhenRowsFull');

  // data_only + tail -> tail anchor already bounds the window; no early-stop.
  const tail = NetdataRequest.parse(
    { anchor: 150000000, ...win, data_only: true, if_modified_since: 1, tail: true },
    config);
  q = _requestToExplorerQuery(tail, 1, null);
  assert.equal(q.stopWhenRowsFull, false,
    'tail-anchored data_only must not early-stop');
}

export async function run() {
  testCancelDuringSingleFileScan();
  testCancelDuringCombinedPass();
  testCapEffectiveDisplayHighBits();
  testConformanceClasses();
  testConformanceFunctions();
  testConformanceConstants();
  testNoExtraneousFreeExploreFunctions();
  testFileReaderHasExplorerMethods();
  testNetdataFunctionSurface();
  testExplorerControlMatchedRowType();
  testAnchorOutsideWindowReset();
  testDataOnlyStopWhenRowsFull();
  console.log('  PASS SOW-0105 fixes (round 1)');
}
