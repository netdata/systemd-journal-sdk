import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  NetdataJournalFunction,
  NetdataFunctionConfig,
  NetdataFunctionRunOptions,
  NetdataFunctionProgress,
  NetdataRequest,
  NetdataFunctionState,
  NetdataJournalFileMetadata,
  SystemdJournalProfile,
  NETDATA_ACCEPTED_PARAMS,
  _requestToExplorerQuery,
} from '../../src/lib/netdata.js';
import { Writer } from '../../src/lib/writer.js';

const enc = (s) => Buffer.from(s, 'utf8');

function makeTwoMachineDir(tmp, {
  baseTimeUsec = 1700000000_000000n,
  countA = 5,
  countB = 3,
  machineIdA = Buffer.alloc(16, 0x11),
  machineIdB = Buffer.alloc(16, 0x22),
  bootIdA = Buffer.alloc(16, 0xaa),
  bootIdB = Buffer.alloc(16, 0xbb),
} = {}) {
  const subA = join(tmp, 'aabbccdd-1111-1111-1111-111111111111');
  const subB = join(tmp, 'eeff00aa-2222-2222-2222-222222222222');
  mkdirSync(subA, { recursive: true });
  mkdirSync(subB, { recursive: true });
  const fileA = join(subA, 'system.journal');
  const fileB = join(subB, 'system.journal');
  const wA = Writer.create(fileA, {
    machineId: machineIdA,
    bootId: bootIdA,
    seqnumId: Buffer.alloc(16, 0x33),
  });
  for (let i = 0; i < countA; i++) {
    wA.append([
      { name: 'MESSAGE', value: enc(`from-a-${i}`) },
      { name: 'PRIORITY', value: enc('3') },
      { name: 'SERVICE', value: enc('svc-a') },
    ], { realtimeUsec: baseTimeUsec + BigInt(i) * 1000n });
  }
  wA.close();
  const wB = Writer.create(fileB, {
    machineId: machineIdB,
    bootId: bootIdB,
    seqnumId: Buffer.alloc(16, 0x33),
  });
  for (let i = 0; i < countB; i++) {
    wB.append([
      { name: 'MESSAGE', value: enc(`from-b-${i}`) },
      { name: 'PRIORITY', value: enc('6') },
      { name: 'SERVICE', value: enc('svc-b') },
    ], { realtimeUsec: baseTimeUsec + 100000n + BigInt(i) * 1000n });
  }
  wB.close();
  return { dir: tmp, fileA, fileB };
}

function makeHighRowSamplingDir(tmp) {
  const sub = join(tmp, 'aabbccdd111111111111111111111111');
  mkdirSync(sub, { recursive: true });
  const file = join(sub, 'system.journal');
  const baseTimeUsec = 1700000000_000000n;
  const w = Writer.create(file, {
    machineId: Buffer.from('aabbccdd111111111111111111111111', 'hex'),
    bootId: Buffer.alloc(16, 0x11),
    seqnumId: Buffer.alloc(16, 0x22),
  });
  for (let i = 0; i < 5000; i++) {
    w.append([
      { name: 'MESSAGE', value: enc(`high-row-${i}`) },
      { name: 'PRIORITY', value: enc(String(3 + (i % 4))) },
      { name: 'SERVICE', value: enc(`svc-${i % 6}`) },
    ], { realtimeUsec: baseTimeUsec + BigInt(i) * 1000n });
  }
  w.close();
  return { dir: tmp, file };
}

function withTmp(fn) {
  const dir = mkdtempSync(join(tmpdir(), 'netdata-2c-'));
  try { fn(dir); }
  finally { try { rmSync(dir, { recursive: true }); } catch {} }
}

// ---------------------------------------------------------------------------
// State / metadata defaults
// ---------------------------------------------------------------------------

function testStateFieldDefaults() {
  const state = new NetdataFunctionState();
  assert.equal(state.fileMetadata('/no/such/path'), null);
  state.updateFileJournalVsRealtimeDeltaUsec('/no', 123);
}

function testMetadataDefaults() {
  const meta = new NetdataJournalFileMetadata();
  assert.equal(meta.sourceType, null);
  assert.equal(meta.sourceName, null);
  assert.equal(meta.fileLastModifiedUsec, null);
  assert.equal(meta.msgFirstRealtimeUsec, null);
  assert.equal(meta.msgLastRealtimeUsec, null);
  assert.equal(meta.journalVsRealtimeDeltaUsec, null);
}

function testDefaultProgressInterval250ms() {
  const opts = new NetdataFunctionRunOptions();
  assert.equal(opts.progressInterval, 0.25);
}

function testFromTimeoutSecondsZero() {
  const opts = NetdataFunctionRunOptions.fromTimeoutSeconds(0);
  assert.equal(opts.timeout, null);
}

// ---------------------------------------------------------------------------
// data_only shape
// ---------------------------------------------------------------------------

function testDataOnlyDropsFacets() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      facets: ['PRIORITY'],
    });
    assert.ok(response._request.data_only);
    assert.equal('facets_delta' in response, false);
    assert.equal('histogram_delta' in response, false);
    assert.equal('items_delta' in response, false);
    assert.equal('facets' in response, false);
    assert.equal('histogram' in response, false);
    assert.equal('items' in response, false);
    assert.ok(response.expires > 0);
  });
}

function testDataOnlyDropsColumnsEnvelope() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
    });
    assert.equal('available_histograms' in response, false);
    assert.equal('last_modified' in response, false);
  });
}

// ---------------------------------------------------------------------------
// Delta keys
// ---------------------------------------------------------------------------

function testDeltaKeysPresent() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.ok('facets_delta' in response);
    assert.ok('histogram_delta' in response);
    assert.ok('items_delta' in response);
    assert.equal('facets' in response, false);
    assert.equal('histogram' in response, false);
    const items = response.items_delta;
    for (const key of ['evaluated', 'matched', 'unsampled', 'estimated', 'returned', 'max_to_return', 'before', 'after']) {
      assert.ok(key in items, `items_delta missing key ${key}`);
    }
    const priority = response.facets_delta.find(f => f.id === 'PRIORITY');
    assert.ok(priority);
    const priorityMap = {};
    for (const o of priority.options) priorityMap[o.name] = o.count;
    assert.equal(priorityMap.error, 5);
    assert.equal(priorityMap.info, 3);
  });
}

// ---------------------------------------------------------------------------
// data_only omits full metadata keys (Rust L702-720)
// ---------------------------------------------------------------------------

function testDataOnlyOmitsFullMetadataKeys() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 50, countB: 30 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.equal(response.status, 200);
    for (const forbidden of ['accepted_params', 'default_sort_column', 'default_charts', 'message', 'update_every', 'help']) {
      assert.equal(forbidden in response, false, `data_only must not carry ${forbidden}`);
    }
  });
}

function testDataOnlyDeltaUsesDeltaVariantKeys() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.equal(response.status, 200);
    for (const forbidden of ['accepted_params', 'default_sort_column', 'default_charts', 'message', 'update_every', 'help']) {
      assert.equal(forbidden in response, false);
    }
    assert.ok('facets_delta' in response);
    assert.ok('histogram_delta' in response);
    assert.ok('items_delta' in response);
    assert.equal('facets' in response, false);
    assert.equal('histogram' in response, false);
    assert.equal('items' in response, false);
    assert.ok(response.available_histograms.length >= 1);
  });
}

function testDataOnlyDeltaDisablesStopWhenRowsFull() {
  const config = NetdataFunctionConfig.systemdJournal();
  let request = NetdataRequest.parse({
    after: 1700000000,
    before: 1700000010,
    data_only: true,
    delta: true,
    direction: 'backward',
    last: 5,
    facets: ['PRIORITY'],
    histogram: 'PRIORITY',
  }, config);
  let query = _requestToExplorerQuery(request, 1, null);
  assert.equal(query.stopWhenRowsFull, false);

  request = NetdataRequest.parse({
    after: 1700000000,
    before: 1700000010,
    data_only: true,
    direction: 'backward',
    last: 5,
    facets: ['PRIORITY'],
    histogram: 'PRIORITY',
  }, config);
  query = _requestToExplorerQuery(request, 1, null);
  assert.equal(query.stopWhenRowsFull, true);
}

// ---------------------------------------------------------------------------
// 304 short-circuit
// ---------------------------------------------------------------------------

function testIfModifiedSinceUnchangedReturns304() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      if_modified_since: 1_700_000_000_500_000,
      after: 1577836800,
      before: 1893456000,
    });
    assert.equal(response.status, 304);
    assert.ok('errorMessage' in response);
    assert.equal('error' in response, false, 'must use errorMessage, not error');
  });
}

function testIfModifiedSinceNewerRunsScan() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      if_modified_since: 0,
      after: 1577836800,
      before: 1893456000,
    });
    assert.equal(response.status, 200);
  });
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

function testSamplingMathSmallBudget() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 50, countB: 30 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      sampling: 2,
      last: 2,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
    });
    assert.ok('_sampling' in response);
    const sampling = response._sampling;
    assert.equal(sampling.enabled, true);
    for (const key of ['sampled', 'unsampled', 'estimated']) {
      assert.ok(key in sampling, `_sampling missing ${key}`);
    }
    const stats = response._stats.sdk_explorer;
    const totalSampling = sampling.sampled + sampling.unsampled + sampling.estimated;
    assert.ok(totalSampling > 0, 'sampling must trigger');
    assert.ok(
      totalSampling + Number(stats.rows_returned) >= Number(stats.rows_matched),
      'sampling plus returned candidates covers rows_matched',
    );
  });
}

function testSamplingHighRowWindowMatchesRustAccounting() {
  withTmp((tmp) => {
    makeHighRowSamplingDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      after: 1700000000,
      before: 1700000005,
      facets: ['PRIORITY', 'SERVICE'],
      histogram: 'PRIORITY',
      last: 5,
      sampling: 20,
      slice: true,
    });
    assert.equal(response.status, 200);
    assert.deepEqual(response.items, {
      evaluated: 4604,
      matched: 4604,
      unsampled: 34,
      estimated: 4554,
      returned: 5,
      max_to_return: 5,
      before: 0,
      after: 11,
    });
    assert.deepEqual(response._sampling, {
      enabled: true,
      sampled: 11,
      unsampled: 35,
      estimated: 4554,
    });
    const stats = response._stats.sdk_explorer;
    assert.equal(stats.rows_examined, 16);
    assert.equal(stats.rows_matched, 4604);
    assert.equal(stats.rows_estimated, 4554);
    assert.equal(stats.rows_unsampled, 34);
  });
}

function testSamplingHighRowAnchorItemsMatchRustAccounting() {
  withTmp((tmp) => {
    makeHighRowSamplingDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const cases = [
      [
        {
          after: 1700000000,
          before: 1700000005,
          anchor: 1700000004990000,
          facets: ['PRIORITY', 'SERVICE'],
          histogram: 'PRIORITY',
          last: 5,
          sampling: 20,
          slice: true,
        },
        {
          evaluated: 4604,
          matched: 4594,
          unsampled: 34,
          estimated: 4554,
          returned: 5,
          max_to_return: 5,
          before: 10,
          after: 1,
        },
      ],
      [
        {
          after: 1700000000,
          before: 1700000005,
          anchor: 1700000000010000,
          direction: 'forward',
          facets: ['PRIORITY', 'SERVICE'],
          histogram: 'PRIORITY',
          last: 5,
          sampling: 20,
          slice: true,
        },
        {
          evaluated: 4604,
          matched: 4593,
          unsampled: 34,
          estimated: 4554,
          returned: 5,
          max_to_return: 5,
          before: 0,
          after: 11,
        },
      ],
    ];
    for (const [request, expected] of cases) {
      const response = fn.runDirectoryRequestJson(tmp, request);
      assert.equal(response.status, 200);
      assert.deepEqual(response.items, expected);
      assert.deepEqual(response._sampling, {
        enabled: true,
        sampled: 11,
        unsampled: 35,
        estimated: 4554,
      });
    }
  });
}

function testSamplingZeroKeepsLegacyFields() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      sampling: 0,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
    });
    assert.equal('_sampling' in response, false);
  });
}

function testSamplingSkippedInDataOnlyWithoutDelta() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      sampling: 20,
      after: 1577836800,
      before: 1893456000,
    });
    assert.equal('_sampling' in response, false);
  });
}

// ---------------------------------------------------------------------------
// Run options: progress, cancellation, timeout
// ---------------------------------------------------------------------------

function testProgressCallbackFires() {
  const seen = [];
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 3, countB: 2 });
    const fn = NetdataJournalFunction.systemdJournal();
    const opts = new NetdataFunctionRunOptions({ progressCallback: (p) => seen.push(p) });
    const response = fn.runDirectoryRequestJsonWithOptions(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
    }, opts);
    assert.equal(response.status, 200);
  });
  assert.ok(seen.length >= 2, `expected >= 2 progress events, got ${seen.length}`);
  const first = seen[0];
  assert.ok(first.currentFile >= 1);
  assert.ok(first.totalFiles >= 2);
  assert.ok(first.elapsed >= 0);
}

function testCancellationCallbackShortCircuits() {
  let n = 0;
  const cancel = () => { n++; return n > 1; };
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 4, countB: 4 });
    const fn = NetdataJournalFunction.systemdJournal();
    const opts = new NetdataFunctionRunOptions({ cancellationCallback: cancel });
    const response = fn.runDirectoryRequestJsonWithOptions(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
    }, opts);
    assert.equal(response.status, 499);
  });
}

function testTimeoutZeroMeansNoDeadline() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 2, countB: 2 });
    const fn = NetdataJournalFunction.systemdJournal();
    const opts = NetdataFunctionRunOptions.fromTimeoutSeconds(0);
    assert.equal(opts.timeout, null);
    const response = fn.runDirectoryRequestJsonWithOptions(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
    }, opts);
    assert.equal(response.status, 200);
  });
}

// ---------------------------------------------------------------------------
// State hook
// ---------------------------------------------------------------------------

function testStateFileMetadataOverridesClassification() {
  let calls = 0;
  class TestState extends NetdataFunctionState {
    fileMetadata(path) {
      calls++;
      return new NetdataJournalFileMetadata({ msgLastRealtimeUsec: 0 });
    }
  }
  const state = new TestState();
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const opts = new NetdataFunctionRunOptions({ state });
    const response = fn.runDirectoryRequestJsonWithOptions(tmp, {
      data_only: true,
      if_modified_since: 1,
      after: 1577836800,
      before: 1893456000,
    }, opts);
    assert.equal(response.status, 304);
    assert.ok(calls > 0, 'state.file_metadata must have been called');
  });
}

function testStateLearnsRealtimeDelta() {
  const updates = [];
  class TestState extends NetdataFunctionState {
    updateFileJournalVsRealtimeDeltaUsec(path, delta) {
      updates.push({ path, delta });
    }
  }
  const state = new TestState();
  const baseUsec = 1700000000_000000n;
  const sourceOffsetUsec = 6_000_000n;
  withTmp((tmp) => {
    const sub = join(tmp, 'aa-bb-cc-dd-1111-111111111111');
    mkdirSync(sub, { recursive: true });
    const file = join(sub, 'system.journal');
    const w = Writer.create(file, {
      machineId: Buffer.alloc(16, 0x11),
      bootId: Buffer.alloc(16, 0xaa),
      seqnumId: Buffer.alloc(16, 0x33),
    });
    for (let i = 0; i < 3; i++) {
      const rt = baseUsec + BigInt(i) * 1000n;
      w.append([
        { name: 'MESSAGE', value: enc(`msg-${i}`) },
        { name: 'PRIORITY', value: enc('3') },
        { name: '_SOURCE_REALTIME_TIMESTAMP', value: enc(String(rt - sourceOffsetUsec)) },
      ], { realtimeUsec: rt });
    }
    w.close();
    const fn = NetdataJournalFunction.systemdJournal();
    const opts = new NetdataFunctionRunOptions({ state });
    fn.runDirectoryRequestJsonWithOptions(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
    }, opts);
  });
  assert.ok(updates.length > 0, 'state must have been updated with learned delta');
}

// ---------------------------------------------------------------------------
// Full response carries full metadata keys
// ---------------------------------------------------------------------------

function testFullResponseCarriesFullMetadataKeys() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.equal(response.status, 200);
    for (const required of ['accepted_params', 'default_sort_column', 'default_charts', 'message', 'update_every', 'help', 'last_modified']) {
      assert.ok(required in response, `full response must carry ${required}`);
    }
    assert.ok(response.accepted_params.length >= NETDATA_ACCEPTED_PARAMS.length);
  });
}

// ---------------------------------------------------------------------------
// Delta facet option names use profile rendering
// ---------------------------------------------------------------------------

function testDeltaFacetsUseProfileRenderedNames() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 100,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.ok('facets_delta' in response);
    const priority = response.facets_delta.find(f => f.id === 'PRIORITY');
    assert.ok(priority);
    const names = new Set(priority.options.map(o => o.name));
    assert.ok(names.has('error'), 'must have error, not 3');
    assert.ok(names.has('info'), 'must have info, not 6');
    assert.equal(names.has('3'), false);
    assert.equal(names.has('6'), false);
  });
}

function testNonDeltaFacetsUseProfileRenderedNames() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 100,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
    });
    assert.ok('facets' in response);
    const priority = response.facets.find(f => f.id === 'PRIORITY');
    assert.ok(priority);
    const names = new Set(priority.options.map(o => o.name));
    assert.ok(names.has('error'));
    assert.ok(names.has('info'));
    assert.equal(names.has('3'), false);
    assert.equal(names.has('6'), false);
  });
}

// ---------------------------------------------------------------------------
// Delta histogram integer values
// ---------------------------------------------------------------------------

function testDeltaHistogramActualDimensionsAreInteger() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 100,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.ok('histogram_delta' in response);
    const chart = response.histogram_delta.chart;
    const data = chart.result.data;
    assert.ok(data.length > 0);
    for (const point of data) {
      for (const entry of point.slice(1)) {
        assert.ok(Array.isArray(entry), `histogram entry must be array, got ${typeof entry}`);
        assert.equal(entry.length, 3);
        const value = entry[0];
        if (value != null) {
          assert.equal(typeof value, 'number', `histogram dimension value must be number, got ${typeof value}: ${value}`);
          assert.ok(Number.isInteger(value), `histogram dimension value must be integer: ${value}`);
        }
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Tail items.after includes +1 for exclusive anchor
// ---------------------------------------------------------------------------

function testTailItemsAfterIncludesAnchor() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const base = 1_700_000_000_000_000;
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      delta: true,
      tail: true,
      if_modified_since: base - 1_000_000,
      anchor: base + 2 * 1000,
      after: 1577836800,
      before: 1893456000,
      last: 100,
      facets: ['PRIORITY'],
    });
    assert.equal(response.status, 200);
    const items = response.items_delta;
    const returned = response.data.length;
    const rowsMatched = Number(response._stats.sdk_explorer.rows_matched);
    const rawAfter = rowsMatched > returned ? rowsMatched - returned : 0;
    assert.equal(items.after, rawAfter + 1, 'items_delta.after must include +1 for exclusive anchor');
  });
}

function testNonTailItemsAfterHasNoAnchorPlusOne() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 3,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
    });
    assert.equal(response.status, 200);
    const items = response.items;
    const returned = response.data.length;
    const rowsMatched = Number(response._stats.sdk_explorer.rows_matched);
    let rawAfter = rowsMatched - returned;
    if (rawAfter < 0) rawAfter = 0;
    assert.equal(items.after, rawAfter);
  });
}

// ---------------------------------------------------------------------------
// Filtered tail: empty 200 vs unfiltered 304
// ---------------------------------------------------------------------------

function testFilteredTailNoMatchReturnsEmpty200() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const base = 1_700_000_000_000_000;
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      tail: true,
      if_modified_since: base - 1_000_000,
      anchor: base + 4 * 1000,
      after: 1577836800,
      before: 1893456000,
      last: 100,
      selections: { SERVICE: ['svc-a'] },
    });
    assert.equal(response.status, 200);
    assert.ok(Array.isArray(response.data));
    assert.equal(response.data.length, 0);
    for (const key of ['status', 'type', 'columns', 'data', 'expires']) {
      assert.ok(key in response, `200 empty envelope missing key ${key}`);
    }
  });
}

function testUnfilteredTailNoNewDataReturns304() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const base = 1_700_000_000_000_000;
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      tail: true,
      if_modified_since: base + 200_000,
      after: 1577836800,
      before: 1893456000,
      last: 100,
    });
    assert.equal(response.status, 304);
  });
}

// ---------------------------------------------------------------------------
// last_modified present unless data_only without tail
// ---------------------------------------------------------------------------

function testLastModifiedPresentUnlessDataOnly() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const fullResponse = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      facets: ['PRIORITY'],
    });
    assert.ok('last_modified' in fullResponse);

    const dataOnlyResponse = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      if_modified_since: 0,
      facets: ['PRIORITY'],
    });
    assert.equal('last_modified' in dataOnlyResponse, false, 'data_only without tail must not have last_modified');
  });
}

// ---------------------------------------------------------------------------
// Available histograms content
// ---------------------------------------------------------------------------

function testNonDataOnlyListMatchesRequestFacets() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: false,
      facets: ['PRIORITY', 'SERVICE', 'SYSLOG_IDENTIFIER'],
      histogram: 'PRIORITY',
    });
    assert.equal(response.status, 200);
    const ids = response.available_histograms.map(e => e.id);
    assert.deepEqual(ids, ['PRIORITY', 'SERVICE', 'SYSLOG_IDENTIFIER']);
    for (const entry of response.available_histograms) {
      assert.ok('order' in entry);
      assert.equal(typeof entry.order, 'number');
      assert.ok(entry.order >= 1);
    }
  });
}

function testDataOnlyAppendsHistogramField() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY', 'SERVICE'],
      histogram: 'SYSLOG_IDENTIFIER',
    });
    assert.equal(response.status, 200);
    const ids = response.available_histograms.map(e => e.id);
    assert.deepEqual(ids, ['PRIORITY', 'SERVICE', 'SYSLOG_IDENTIFIER']);
  });
}

function testDataOnlyDedupesWhenHistogramInFacets() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 10,
      after: 1577836800,
      before: 1893456000,
      data_only: true,
      delta: true,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    const ids = response.available_histograms.map(e => e.id);
    assert.deepEqual(ids, ['PRIORITY']);
  });
}

// ---------------------------------------------------------------------------
// Histogram bucket grid value-pinning (SOW-0105 comparator fix 4)
// ---------------------------------------------------------------------------

function testHistogramBucketKeysAreGridSnapped() {
  withTmp((tmp) => {
    const sub = join(tmp, 'aabbccdd111111111111111111111111');
    mkdirSync(sub, { recursive: true });
    const file = join(sub, 'system.journal');
    const baseSec = 1666569600;
    const w = Writer.create(file, {
      machineId: Buffer.alloc(16, 0x11),
      bootId: Buffer.alloc(16, 0xaa),
      seqnumId: Buffer.alloc(16, 0x33),
    });
    for (let i = 0; i < 5; i++) {
      w.append([
        { name: 'MESSAGE', value: enc(`msg-${i}`) },
        { name: 'PRIORITY', value: enc(String(3 + (i % 3))) },
      ], { realtimeUsec: (baseSec + 10 + i * 600) * 1_000_000 });
    }
    w.close();
    const fn = NetdataJournalFunction.systemdJournal();
    const after = baseSec + 1;
    const before = baseSec + 15000;
    const snapAfterUsec = (after - (after % 60)) * 1_000_000;
    const snapBeforeUsec = (before - (before % 60) + 60) * 1_000_000;
    const widthUsec = 60 * 1_000_000;
    const bucketCount = ((snapBeforeUsec - snapAfterUsec) / widthUsec) + 1;
    const expectedBucketKeys = [];
    for (let i = 0; i < bucketCount; i++) {
      expectedBucketKeys.push((snapAfterUsec + i * widthUsec) / 1000);
    }
    const response = fn.runDirectoryRequestJson(tmp, {
      after, before,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    });
    assert.equal(response.status, 200);
    const chart = response.histogram.chart;
    const data = chart.result.data;
    assert.ok(data.length > 0, 'histogram data must not be empty');
    const actualBucketKeys = data.map(p => p[0]);
    assert.deepEqual(actualBucketKeys, expectedBucketKeys, 'bucket keys must match grid-snapped multiples');
    const firstBucketMs = actualBucketKeys[0];
    assert.equal(firstBucketMs % 60000, 0, 'first bucket key must be a round grid multiple');
  });
}

function testPriorityFacetSortMatchesRustForNonU8Values() {
  withTmp((tmp) => {
    const sub = join(tmp, 'aabbccdd111111111111111111111111');
    mkdirSync(sub, { recursive: true });
    const file = join(sub, 'system.journal');
    const w = Writer.create(file, {
      machineId: Buffer.alloc(16, 0x11),
      bootId: Buffer.alloc(16, 0xaa),
      seqnumId: Buffer.alloc(16, 0x33),
    });
    const priorities = ['3', 'abc', '300', '6'];
    for (let i = 0; i < priorities.length; i++) {
      w.append([
        { name: 'MESSAGE', value: enc('priority-edge-order') },
        { name: 'PRIORITY', value: enc(priorities[i]) },
      ], { realtimeUsec: 1700000000_000000n + BigInt(i) * 1_000_000n });
    }
    w.close();
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      after: 1699999999,
      before: 1700001000,
      last: 50,
      facets: ['PRIORITY'],
    });
    const priority = response.facets.find(f => f.id === 'PRIORITY');
    assert.ok(priority);
    assert.deepEqual(priority.options.map(o => o.id), ['300', 'abc', '3', '6']);
  });
}

function testHistogramEmptyDataProducesGridSnappedBuckets() {
  withTmp((tmp) => {
    const sub = join(tmp, 'aabbccdd111111111111111111111111');
    mkdirSync(sub, { recursive: true });
    const file = join(sub, 'system.journal');
    const baseSec = 1666569600;
    const w = Writer.create(file, {
      machineId: Buffer.alloc(16, 0x11),
      bootId: Buffer.alloc(16, 0xaa),
      seqnumId: Buffer.alloc(16, 0x33),
    });
    for (let i = 0; i < 3; i++) {
      w.append([
        { name: 'MESSAGE', value: enc(`early-${i}`) },
        { name: 'PRIORITY', value: enc('4') },
      ], { realtimeUsec: (baseSec + 1 + i * 10) * 1_000_000 });
    }
    w.close();
    const fn = NetdataJournalFunction.systemdJournal();
    const options = new NetdataFunctionRunOptions();
    options._injectableNow = (baseSec + 1000000) * 1000;
    const response = fn.runDirectoryRequestJsonWithOptions(tmp, {
      after: baseSec + 5000,
      before: baseSec + 5200,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
    }, options);
    assert.equal(response.status, 200);
    const histogram = response.histogram;
    assert.ok(histogram != null);
    assert.ok(histogram.chart != null);
    const view = histogram.chart.view;
    assert.equal(view.dimensions.names.length, 0, 'empty window has empty dimension names');
    const gridAfterSec = Math.floor((baseSec + 5000));
    const gridBeforeSec = Math.floor((baseSec + 5200) + 1);
    assert.equal(view.after, gridAfterSec);
    assert.equal(view.before, gridBeforeSec);
  });
}

// ---------------------------------------------------------------------------
// Compact 304 envelope key set (SOW-0105 comparator fix 4)
// ---------------------------------------------------------------------------

function test304EnvelopeHasOnlyStatusAndErrorMessage() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp);
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      data_only: true,
      if_modified_since: 1_700_000_000_500_000,
      after: 1577836800,
      before: 1893456000,
    });
    assert.equal(response.status, 304);
    const keys = Object.keys(response).sort();
    assert.deepEqual(keys, ['errorMessage', 'status'], '304 envelope must have exactly status and errorMessage');
  });
}

// ---------------------------------------------------------------------------
// Filtered-field facet vocabulary zero-count post-passes
// (SOW-0105 comparator fix 5 — exclude-own-field-filter facet semantics)
// ---------------------------------------------------------------------------

function testFilterValueSurfacesInFacetEvenWithZeroMatches() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 0, countB: 0 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 5,
      after: 1577836800,
      before: 1893456000,
      data_only: false,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
      selections: { PRIORITY: ['3'] },
    });
    assert.equal(response.status, 200);
    const priorityFacet = response.facets.find(f => f.id === 'PRIORITY');
    assert.ok(priorityFacet, 'PRIORITY facet must exist');
    const priorityMap = {};
    for (const o of priorityFacet.options) priorityMap[o.id] = o.count;
    assert.ok('3' in priorityMap, '"3" must surface as zero-count from selected filter value');
    assert.equal(priorityMap['3'], 0);
    assert.equal(response.data.length, 0);
  });
}

function testFilterValueSurfacesInHistogramDimensions() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 0, countB: 0 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 5,
      after: 1577836800,
      before: 1893456000,
      data_only: false,
      facets: ['PRIORITY'],
      histogram: 'PRIORITY',
      selections: { PRIORITY: ['3'] },
    });
    assert.equal(response.status, 200);
    const h = response.histogram;
    assert.ok(h != null, 'histogram must exist');
    const ids = h.chart.db.dimensions.ids;
    const names = h.chart.view.dimensions.names;
    assert.ok(ids.includes('3'), 'histogram dimension ids must include "3"');
    assert.ok(names.includes('error'), 'histogram dimension names must include "error"');
    assert.equal(ids.length, names.length);
  });
}

function testFileVocabularyWidensFacetWithUnselectedValues() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 100,
      after: 1577836800,
      before: 1893456000,
      data_only: false,
      facets: ['PRIORITY'],
      selections: { PRIORITY: ['3'] },
    });
    const priorityFacet = response.facets.find(f => f.id === 'PRIORITY');
    assert.ok(priorityFacet);
    const priorityMap = {};
    for (const o of priorityFacet.options) priorityMap[o.id] = o.count;
    assert.equal(priorityMap['3'], 5, 'PRIORITY=3 must have count 5 from file_a');
    assert.equal(priorityMap['6'], 0, 'PRIORITY=6 must surface as zero-count via vocabulary widening');
  });
}

function testMultiFilterExcludesOwnFieldFacetHasRealCounts() {
  withTmp((tmp) => {
    makeTwoMachineDir(tmp, { countA: 5, countB: 3 });
    const fn = NetdataJournalFunction.systemdJournal();
    const response = fn.runDirectoryRequestJson(tmp, {
      last: 100,
      after: 1577836800,
      before: 1893456000,
      data_only: false,
      facets: ['PRIORITY'],
      selections: { PRIORITY: ['3'], SERVICE: ['svc-b'] },
    });
    const priorityFacet = response.facets.find(f => f.id === 'PRIORITY');
    assert.ok(priorityFacet);
    const priorityMap = {};
    for (const o of priorityFacet.options) priorityMap[o.id] = o.count;
    assert.equal(priorityMap['6'], 3,
      'PRIORITY=6 must have real count 3 — file_b rows match SERVICE=svc-b and the own-field filter is excluded');
    assert.equal(priorityMap['3'], 0,
      'PRIORITY=3 must be zero-count — file_a rows do not match SERVICE=svc-b');
  });
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

export async function run() {
  testStateFieldDefaults();
  testMetadataDefaults();
  testDefaultProgressInterval250ms();
  testFromTimeoutSecondsZero();

  testDataOnlyDropsFacets();
  testDataOnlyDropsColumnsEnvelope();
  testDataOnlyOmitsFullMetadataKeys();
  testDataOnlyDeltaUsesDeltaVariantKeys();
  testDataOnlyDeltaDisablesStopWhenRowsFull();

  testDeltaKeysPresent();
  testDeltaFacetsUseProfileRenderedNames();
  testNonDeltaFacetsUseProfileRenderedNames();
  testDeltaHistogramActualDimensionsAreInteger();

  testIfModifiedSinceUnchangedReturns304();
  testIfModifiedSinceNewerRunsScan();
  testFilteredTailNoMatchReturnsEmpty200();
  testUnfilteredTailNoNewDataReturns304();

  testSamplingMathSmallBudget();
  testSamplingHighRowWindowMatchesRustAccounting();
  testSamplingHighRowAnchorItemsMatchRustAccounting();
  testSamplingZeroKeepsLegacyFields();
  testSamplingSkippedInDataOnlyWithoutDelta();

  testProgressCallbackFires();
  testCancellationCallbackShortCircuits();
  testTimeoutZeroMeansNoDeadline();

  testStateFileMetadataOverridesClassification();
  testStateLearnsRealtimeDelta();

  testFullResponseCarriesFullMetadataKeys();
  testLastModifiedPresentUnlessDataOnly();

  testNonDataOnlyListMatchesRequestFacets();
  testDataOnlyAppendsHistogramField();
  testDataOnlyDedupesWhenHistogramInFacets();

  testTailItemsAfterIncludesAnchor();
  testNonTailItemsAfterHasNoAnchorPlusOne();

  testHistogramBucketKeysAreGridSnapped();
  testPriorityFacetSortMatchesRustForNonU8Values();
  testHistogramEmptyDataProducesGridSnappedBuckets();
  test304EnvelopeHasOnlyStatusAndErrorMessage();

  testFilterValueSurfacesInFacetEvenWithZeroMatches();
  testFilterValueSurfacesInHistogramDimensions();
  testFileVocabularyWidensFacetWithUnselectedValues();
  testMultiFilterExcludesOwnFieldFacetHasRealCounts();

  console.log('  PASS netdata chunk 2c (stateful semantics completion)');
}
