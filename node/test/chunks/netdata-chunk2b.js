import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  NETDATA_ACCEPTED_PARAMS,
  NetdataJournalFunction,
  NetdataRequest,
  CombinedResult,
  JournalFileCollection,
  normalizeTimeWindow,
  journalFileSourceType,
  collectJournalFiles,
  NETDATA_SOURCE_TYPE_ALL,
  NETDATA_SOURCE_TYPE_LOCAL_ALL,
  NETDATA_SOURCE_TYPE_REMOTE_ALL,
  NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
  NETDATA_SOURCE_TYPE_LOCAL_USER,
  NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
  NETDATA_SOURCE_TYPE_LOCAL_OTHER,
  NetdataFunctionConfig,
  SystemdJournalProfile,
  SystemdJournalPluginProfile,
  DisplayContext,
  DEFAULT_TIME_WINDOW_SECONDS,
} from '../../src/lib/netdata.js';
import { Writer } from '../../src/lib/writer.js';
import { Direction } from '../../src/lib/explorer.js';

let tmpDir = null;

function setup() {
  tmpDir = mkdtempSync(join(tmpdir(), 'netdata-test-'));
}

function teardown() {
  if (tmpDir) { try { rmSync(tmpDir, { recursive: true }); } catch {} tmpDir = null; }
}

function testFunctionConstructors() {
  const fn1 = NetdataJournalFunction.systemdJournal();
  assert.ok(fn1 instanceof NetdataJournalFunction);
  assert.ok(fn1._profile instanceof SystemdJournalProfile);

  const fn2 = NetdataJournalFunction.systemdJournalPluginCompatible();
  assert.ok(fn2 instanceof NetdataJournalFunction);
  assert.ok(fn2._profile instanceof SystemdJournalPluginProfile);

  const customCfg = new NetdataFunctionConfig({ sourceSelectorName: '', sourceSelectorHelp: '' });
  const fn3 = NetdataJournalFunction.new(customCfg, new SystemdJournalProfile());
  assert.equal(fn3._config.sourceSelectorName, 'Journal Sources');
  assert.equal(fn3._config.sourceSelectorHelp, 'Select the logs source to query');
}

function testInfoResponse() {
  const fn = NetdataJournalFunction.systemdJournal();
  const injectableNow = 1700000000000;
  const result = fn.runDirectoryRequestJsonWithOptions(tmpDir, { info: true }, { _injectableNow: injectableNow });
  assert.equal(result.status, 200);
  assert.equal(result.type, 'table');
  assert.ok(result.accepted_params);
  assert.equal(result.accepted_params.length, 16);
  assert.ok(result.required_params);
  assert.equal(result.required_params[0].id, '__logs_sources');
  assert.equal(result.versions.netdata_function_api, 1);
  assert.ok(result.pagination);
  assert.equal(result.pagination.enabled, true);
  assert.equal(result.pagination.key, 'anchor');
  assert.equal(result.show_ids, true);
  assert.equal(result.has_history, true);
}

function testLogsSourcesResponse() {
  const fn = NetdataJournalFunction.systemdJournal();
  const injectableNow = 1700000000000;
  const result = fn.runDirectoryRequestJsonWithOptions(
    tmpDir,
    { __logs_sources: true },
    { _injectableNow: injectableNow },
  );
  assert.equal(result.status, 200);
  assert.equal(result.type, 'multiselect');
  assert.equal(result.id, '__logs_sources');
  assert.ok(result.options);
  assert.ok(result.name);
  assert.ok(result.help);
}

function testSourceTypeClassification() {
  assert.equal(
    journalFileSourceType('/var/log/journal/system.journal'),
    NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
  );
  assert.equal(
    journalFileSourceType('/var/log/journal/user-1000.journal'),
    NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_USER,
  );
  assert.equal(
    journalFileSourceType('/var/log/journal/remote/remote-host.journal'),
    NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_REMOTE_ALL,
  );
  assert.equal(
    journalFileSourceType('/var/log/journal/abc123.myns/other.journal'),
    NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
  );
  assert.equal(
    journalFileSourceType('/var/log/journal/custom.journal'),
    NETDATA_SOURCE_TYPE_ALL | NETDATA_SOURCE_TYPE_LOCAL_ALL | NETDATA_SOURCE_TYPE_LOCAL_OTHER,
  );
}

function testNormalizeTimeWindow() {
  const now = 1700000000;
  const [a, b] = normalizeTimeWindow(null, null, null, now * 1000);
  assert.ok(a > 0);
  assert.ok(b > 0);
  assert.ok(b > a);
  assert.ok(b - a <= (DEFAULT_TIME_WINDOW_SECONDS + 1) * 1_000_000);

  const [a2, b2] = normalizeTimeWindow(null, 0, 0, now * 1000);
  assert.ok(a2 > 0);
  assert.ok(b2 > 0);
}

function testCollectJournalFiles() {
  const collection = collectJournalFiles(tmpDir);
  assert.ok(collection instanceof JournalFileCollection);
  assert.ok(Array.isArray(collection.files));
  assert.equal(typeof collection.skipped, 'number');
  assert.ok(Array.isArray(collection.errors));
}

function testDiscoverLimits() {
  const deepDir = join(tmpDir, 'deep');
  mkdirSync(deepDir, { recursive: true });
  for (let i = 0; i < 10; i++) {
    mkdirSync(join(deepDir, `l${i}`), { recursive: true });
    writeFileSync(join(deepDir, `l${i}`, `f${i}.journal`), 'not-a-journal');
  }
  const collection = collectJournalFiles(tmpDir);
  assert.ok(collection.files.length <= 10 + 1);
}

function writeSyntheticJournal(dir, name, entries, machineId = 'aabbccdd11223344aabbccdd11223344') {
  const path = join(dir, name);
  const w = Writer.create(path, { machineId });
  for (const entry of entries) {
    w.append(entry.fields, { realtimeUsec: entry.realtimeUsec });
  }
  w.close();
  return path;
}

async function testFullDataRequestOnMultiFile() {
  const dir = mkdtempSync(join(tmpdir(), 'netdata-multi-'));
  try {
    const enc = (s) => Buffer.from(s, 'utf8');
    const nowUsec = 1700000000_000000n;
    const entries1 = [];
    const entries2 = [];
    for (let i = 0; i < 5; i++) {
      entries1.push({
        realtimeUsec: nowUsec - BigInt(i * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`file1-msg-${i}`) },
          { name: 'PRIORITY', value: enc('6') },
          { name: '_HOSTNAME', value: enc('host1') },
          { name: 'SYSLOG_IDENTIFIER', value: enc('test1') },
          { name: '_PID', value: enc('1000') },
        ],
      });
      entries2.push({
        realtimeUsec: nowUsec - BigInt((i + 5) * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`file2-msg-${i}`) },
          { name: 'PRIORITY', value: enc('3') },
          { name: '_HOSTNAME', value: enc('host2') },
          { name: 'SYSLOG_IDENTIFIER', value: enc('test2') },
          { name: '_PID', value: enc('2000') },
        ],
      });
    }
    writeSyntheticJournal(dir, 'system.journal', entries1);
    writeSyntheticJournal(dir, 'system2.journal', entries2);

    const fn = NetdataJournalFunction.systemdJournal();
    const injectableNow = Number(nowUsec / 1000n);
    const result = fn.runDirectoryRequestJsonWithOptions(
      dir,
      { after: 0, before: 0 },
      { _injectableNow: injectableNow },
    );

    assert.equal(result.status, 200, `expected 200, got ${result.status}: ${JSON.stringify(result).slice(0, 200)}`);
    assert.equal(result.type, 'table');
    assert.ok(result.columns, 'must have columns');
    assert.ok(result.data, 'must have data');
    assert.ok(result._stats, 'must have _stats');
    assert.ok(result._stats.sdk_explorer, 'must have sdk_explorer stats');
    assert.ok(result._journal_files, 'must have _journal_files');
    assert.ok(result.pagination, 'must have pagination');
    assert.equal(result.pagination.enabled, true);
    assert.equal(result.show_ids, true);
    assert.equal(result.has_history, true);
    assert.ok('_request' in result);

    assert.ok(result.data.length > 0, 'data must have rows');

    for (const row of result.data) {
      assert.ok(Array.isArray(row), 'each data row must be array');
      const tsIdx = Object.keys(result.columns).indexOf('timestamp');
      if (tsIdx >= 0) {
        const ts = row[0];
        assert.equal(typeof ts, 'number', 'timestamp must be number');
        assert.ok(ts > 0, 'timestamp must be positive');
      }
    }

    assert.ok('accepted_params' in result);
    assert.ok(result.accepted_params.length >= 16);
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

function testStatsMergeSemantics() {
  const combined = new CombinedResult();
  const r1 = {
    rows: [],
    facets: new Map(),
    histogram: null,
    columnFields: new Set(['PRIORITY']),
    stats: (() => {
      const s = { rowsExamined: 100n, rowsMatched: 50n, facetRowsMatched: 50n, rowsReturned: 10n,
        rowsUnsampled: 0n, rowsEstimated: 0n, samplingSampled: 0n, samplingUnsampled: 0n, samplingEstimated: 0n,
        lastRealtimeUsec: 1000n, maxSourceRealtimeDeltaUsec: 5n, dataRefsSeen: 200n, dataRefsSkipped: 10n,
        dataPayloadsLoaded: 100n, dataObjectsClassified: 100n, dataCacheHits: 50n, dataCacheMisses: 50n,
        payloadsDecompressed: 0n, ftsScans: 0n, facetUpdates: 100n, histogramUpdates: 0n,
        returnedRowExpansions: 10n, earlyStopOpportunities: 0n, earlyStops: 0n };
      return s;
    })(),
  };
  const r2 = {
    rows: [],
    facets: new Map(),
    histogram: null,
    columnFields: new Set(['_HOSTNAME']),
    stats: (() => {
      const s = { rowsExamined: 200n, rowsMatched: 80n, facetRowsMatched: 80n, rowsReturned: 20n,
        rowsUnsampled: 0n, rowsEstimated: 0n, samplingSampled: 0n, samplingUnsampled: 0n, samplingEstimated: 0n,
        lastRealtimeUsec: 2000n, maxSourceRealtimeDeltaUsec: 3n, dataRefsSeen: 300n, dataRefsSkipped: 20n,
        dataPayloadsLoaded: 150n, dataObjectsClassified: 150n, dataCacheHits: 60n, dataCacheMisses: 90n,
        payloadsDecompressed: 5n, ftsScans: 0n, facetUpdates: 150n, histogramUpdates: 0n,
        returnedRowExpansions: 20n, earlyStopOpportunities: 1n, earlyStops: 1n };
      return s;
    })(),
  };
  combined.merge('/path/a', r1, Direction.Backward, 200);
  combined.merge('/path/b', r2, Direction.Backward, 200);
  assert.equal(combined.stats.rowsExamined, 300n, 'rowsExamined should be sum');
  assert.equal(combined.stats.rowsMatched, 130n, 'rowsMatched should be sum');
  assert.equal(combined.stats.lastRealtimeUsec, 2000n, 'lastRealtimeUsec should be max');
  assert.equal(combined.stats.maxSourceRealtimeDeltaUsec, 5n, 'maxSourceRealtimeDeltaUsec should be max');
  assert.ok(combined.columnFields.has('PRIORITY'));
  assert.ok(combined.columnFields.has('_HOSTNAME'));
}

function testAcceptedParamsExtension() {
  const fn = NetdataJournalFunction.systemdJournal();
  const injectableNow = 1700000000000;
  const dir = mkdtempSync(join(tmpdir(), 'netdata-ap-'));
  try {
    writeSyntheticJournal(dir, 'system.journal', [{
      realtimeUsec: 1700000000_000000n,
      fields: [
        { name: 'MESSAGE', value: Buffer.from('hello') },
        { name: 'PRIORITY', value: Buffer.from('6') },
      ],
    }]);
    const result = fn.runDirectoryRequestJsonWithOptions(
      dir, { after: 0, before: 0 }, { _injectableNow: injectableNow },
    );
    assert.ok(result.accepted_params.length >= 16);
    try { rmSync(dir, { recursive: true }); } catch {}
  } catch (e) { try { rmSync(dir, { recursive: true }); } catch {} throw e; }
}

function testNDJournalFilePerRow() {
  const fn = NetdataJournalFunction.systemdJournal();
  const injectableNow = 1700000000000;
  const dir = mkdtempSync(join(tmpdir(), 'netdata-ndjf-'));
  try {
    const enc = (s) => Buffer.from(s, 'utf8');
    writeSyntheticJournal(dir, 'system.journal', [{
      realtimeUsec: 1700000000_000000n,
      fields: [
        { name: 'MESSAGE', value: enc('test-nd-journal-file') },
        { name: 'PRIORITY', value: enc('6') },
      ],
    }]);
    const result = fn.runDirectoryRequestJsonWithOptions(
      dir, { after: 0, before: 0 }, { _injectableNow: injectableNow },
    );
    if (result.data && result.data.length > 0) {
      const colKeys = Object.keys(result.columns);
      const ndjIdx = colKeys.indexOf('ND_JOURNAL_FILE');
      if (ndjIdx >= 0) {
        for (const row of result.data) {
          const val = row[ndjIdx];
          assert.ok(val != null, 'ND_JOURNAL_FILE must not be null');
          assert.ok(typeof val === 'string', 'ND_JOURNAL_FILE must be string');
          assert.ok(val.includes('system.journal'), `ND_JOURNAL_FILE must contain filename: ${val}`);
        }
      }
    }
    try { rmSync(dir, { recursive: true }); } catch {}
  } catch (e) { try { rmSync(dir, { recursive: true }); } catch {} throw e; }
}

function testRequestParsing() {
  const config = NetdataFunctionConfig.systemdJournal();
  const injectableNow = 1700000000000;
  const req = NetdataRequest.parse({
    info: false,
    after: -3600,
    before: -60,
    direction: 'backward',
    last: 100,
    query: 'test',
    facets: ['PRIORITY', '_HOSTNAME'],
    histogram: 'PRIORITY',
    if_modified_since: 12345,
    data_only: false,
    delta: false,
    tail: false,
    sampling: 500000,
    anchor: 1700000000000000,
    selections: { __logs_sources: ['all'] },
  }, config, injectableNow);

  assert.equal(req.info, false);
  assert.equal(req.direction, Direction.Backward);
  assert.equal(req.limit, 100);
  assert.equal(req.query, 'test');
  assert.equal(req.histogram, 'PRIORITY');
  assert.equal(req.dataOnly, false);
  assert.equal(req.delta, false);
  assert.equal(req.tail, false);
  assert.equal(req.sampling, 500000);
  assert.ok(req.afterRealtimeUsec != null);
  assert.ok(req.beforeRealtimeUsec != null);
  assert.ok(req.echo);
  assert.equal(req.echo.direction, 'backward');
  assert.equal(req.echo.query, 'test');
}

function testWindowPrefilterSkipsOutOfWindowFiles() {
  const dir = mkdtempSync(join(tmpdir(), 'netdata-prefilter-'));
  try {
    const enc = (s) => Buffer.from(s, 'utf8');
    const baseUsec = 1700000000_000000n;
    const inWindowEntries = [];
    const outOfWindowEntries = [];
    for (let i = 0; i < 3; i++) {
      inWindowEntries.push({
        realtimeUsec: baseUsec + BigInt(i * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`in-window-msg-${i}`) },
          { name: 'PRIORITY', value: enc('6') },
        ],
      });
    }
    for (let i = 0; i < 3; i++) {
      outOfWindowEntries.push({
        realtimeUsec: baseUsec + 250_000_000n + BigInt(i * 1_000_000),
        fields: [
          { name: 'MESSAGE', value: enc(`out-of-window-msg-${i}`) },
          { name: 'PRIORITY', value: enc('3') },
        ],
      });
    }
    writeSyntheticJournal(dir, 'system.journal', inWindowEntries);
    writeSyntheticJournal(dir, 'system2.journal', outOfWindowEntries);

    const fn = NetdataJournalFunction.systemdJournal();
    const injectableNow = Number(baseUsec / 1000n);
    const result = fn.runDirectoryRequestJsonWithOptions(
      dir,
      { after: 1700000000, before: 1700000010, last: 100 },
      { _injectableNow: injectableNow },
    );

    assert.equal(result.status, 200, `expected 200, got ${result.status}: ${JSON.stringify(result).slice(0, 200)}`);
    assert.ok(result._journal_files, 'must have _journal_files');
    assert.equal(result._journal_files.matched, 1, 'only the in-window file should be matched');
    assert.ok(result._journal_files.skipped >= 1, `expected skipped >= 1, got ${result._journal_files.skipped}`);
    assert.ok(Array.isArray(result.data), 'must have data');
    for (const row of result.data) {
      assert.ok(Array.isArray(row), 'row must be array');
    }
  } finally {
    try { rmSync(dir, { recursive: true }); } catch {}
  }
}

export async function run() {
  setup();
  try {
    testFunctionConstructors();
    testRequestParsing();
    testSourceTypeClassification();
    testNormalizeTimeWindow();
    testCollectJournalFiles();
    testDiscoverLimits();
    testStatsMergeSemantics();
    testInfoResponse();
    testLogsSourcesResponse();
    await testFullDataRequestOnMultiFile();
    testAcceptedParamsExtension();
    testNDJournalFilePerRow();
    testWindowPrefilterSkipsOutOfWindowFiles();
    console.log('  PASS netdata chunk 2b (request handling, discovery, envelope)');
  } finally {
    teardown();
  }
}
