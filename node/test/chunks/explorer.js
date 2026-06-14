// Tests for node/src/lib/explorer.js
//
// Ports the intent of python/test_explorer.py (SOW-0104) and the
// upstream Rust unit tests in rust/src/journal/src/explorer.rs.
// Synthetic fixtures are built with the in-repo Node Writer
// (synthetic identities only; never host journal).
//
// Performance contract reminder: the column catalog in
// ExplorerResult.columnFields comes from the FIELD hash-table index
// (FileReader._enumerateFieldsIndexed) — never from row traversal
// (the debug flag is rejected outright as in Rust L2850-2857).

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { Writer, FileReader } from '../../src/index.js';
import {
  Direction,
  ExplorerAnchor,
  ExplorerAnchorKind,
  ExplorerComparison,
  ExplorerControl,
  ExplorerError,
  ExplorerFieldMode,
  ExplorerFilter,
  ExplorerFtsPattern,
  ExplorerQuery,
  ExplorerResult,
  ExplorerSampling,
  ExplorerStats,
  ExplorerStopReason,
  ExplorerStrategy,
  ExplorerUnsupported,
  _combinedSamplingDecision,
  _ExplorerSamplingState,
  UNSET_VALUE,
  DEFAULT_HISTOGRAM_TARGET_BUCKETS,
  DEFAULT_TIME_SLACK_USEC,
  EXPLORER_CONTROL_CHECK_EVERY_ROWS,
  EXPLORER_PROGRESS_INTERVAL_MS,
} from '../../src/lib/explorer.js';

// ---------------------------------------------------------------------------
// Fixture helpers (mirror python/test_explorer.py).
// ---------------------------------------------------------------------------

function makeWriter(path) {
  return Writer.create(path, {
    machineId: Buffer.alloc(16, 0xaa),
    bootId: Buffer.alloc(16, 0xbb),
    seqnumId: Buffer.alloc(16, 0xcc),
  });
}

function writeSimpleEntries(path, entries) {
  const w = makeWriter(path);
  for (const [realtime, fields] of entries) {
    w.append(
      fields.map(([k, v]) => ({ name: k, value: Buffer.isBuffer(v) ? v : Buffer.from(v) })),
      { realtimeUsec: BigInt(realtime) },
    );
  }
  w.close();
}

function writeManyAlternating(path, count) {
  const w = makeWriter(path);
  for (let i = 0; i < count; i++) {
    const service = i % 2 === 0 ? 'even' : 'odd';
    w.append(
      [
        { name: 'MESSAGE', value: Buffer.from(`row-${i}`, 'ascii') },
        { name: 'SERVICE', value: Buffer.from(service) },
      ],
      { realtimeUsec: 1_700_000_000_000_000n + BigInt(i) },
    );
  }
  w.close();
}

function facetCount(result, field, value) {
  // field and value are Buffers; the explorer stores facets and values
  // under hex-string Map keys (the only way to get value-based
  // equality from JavaScript Map, which uses reference equality for
  // object keys).
  const fieldKey = Buffer.isBuffer(field) ? field.toString('hex') : String(field);
  const valueKey = Buffer.isBuffer(value) ? value.toString('hex') : String(value);
  const fieldMap = result.facets.get(fieldKey);
  if (!fieldMap) return undefined;
  return fieldMap.get(valueKey);
}

function fieldHex(name) {
  return Buffer.from(name, 'utf8');
}

function histogramTotalForValue(histogram, value) {
  if (!histogram) return 0n;
  // The explorer stores histogram bucket values under hex-string Map
  // keys (see _incrementCounter for the parity rationale).
  const valueKey = Buffer.isBuffer(value) ? value.toString('hex') : String(value);
  let total = 0n;
  for (const bucket of histogram.buckets) {
    const c = bucket.values.get(valueKey);
    if (c !== undefined) total += c;
  }
  return total;
}

// ---------------------------------------------------------------------------
// Defaults (mirrors Rust ExplorerQuery::default() L116-132).
// ---------------------------------------------------------------------------

function testExplorerQueryDefaultsMatchRust() {
  const q = new ExplorerQuery();
  assert.equal(q.afterRealtimeUsec, null);
  assert.equal(q.beforeRealtimeUsec, null);
  assert.equal(q.anchor.kind, ExplorerAnchorKind.Auto);
  assert.equal(q.anchor.realtimeUsec, 0n);
  assert.equal(q.direction, Direction.Forward);
  assert.equal(q.limit, 200);
  assert.deepEqual(q.filters, []);
  assert.deepEqual(q.facets, []);
  assert.equal(q.histogram, null);
  assert.equal(q.histogramAfterRealtimeUsec, null);
  assert.equal(q.histogramBeforeRealtimeUsec, null);
  assert.equal(q.histogramTargetBuckets, DEFAULT_HISTOGRAM_TARGET_BUCKETS);
  assert.deepEqual(q.ftsTerms, []);
  assert.deepEqual(q.ftsPatterns, []);
  assert.deepEqual(q.ftsNegativePatterns, []);
  assert.equal(q.fieldMode, ExplorerFieldMode.FirstValue);
  assert.equal(q.excludeFacetFieldFilters, true);
  assert.equal(q.useSourceRealtime, true);
  assert.equal(q.realtimeSlackUsec, DEFAULT_TIME_SLACK_USEC);
  assert.equal(q.stopWhenRowsFull, false);
  assert.equal(q.stopWhenRowsFullCheckEvery, Number(1n));
  assert.equal(q.sampling, null);
  assert.equal(q.debugCollectColumnFieldsByRowTraversal, false);
}

// ---------------------------------------------------------------------------
// Builders (return self for chaining, like Rust's consuming builder).
// ---------------------------------------------------------------------------

function testExplorerQueryBuildersReturnSelf() {
  const q = new ExplorerQuery();
  const r1 = q.withFilter('SERVICE', ['api']);
  const r2 = q.withFacet('PRIORITY');
  const r3 = q.withHistogram('PRIORITY');
  const r4 = q.withFtsPattern('alpha');
  const r5 = q.withFtsNegativePattern('boom');
  assert.equal(r1, q);
  assert.equal(r2, q);
  assert.equal(r3, q);
  assert.equal(r4, q);
  assert.equal(r5, q);
  assert.ok(q.filters[0] instanceof ExplorerFilter);
  assert.ok(q.filters[0].field.equals(Buffer.from('SERVICE')));
  assert.deepEqual(
    q.filters[0].values.map((v) => Buffer.from(v).toString('utf8')),
    ['api'],
  );
  assert.equal(q.facets.length, 1);
  assert.ok(q.facets[0].equals(Buffer.from('PRIORITY')));
  assert.ok(q.histogram.equals(Buffer.from('PRIORITY')));
  assert.equal(q.ftsTerms.length, 2);
  assert.equal(q.ftsTerms[0].negative, false);
  assert.equal(q.ftsTerms[1].negative, true);
  assert.equal(q.ftsPatterns.length, 1);
  assert.ok(q.ftsPatterns[0].equals(Buffer.from('alpha')));
  assert.equal(q.ftsNegativePatterns.length, 1);
  assert.ok(q.ftsNegativePatterns[0].equals(Buffer.from('boom')));
}

// ---------------------------------------------------------------------------
// ExplorerFtsPattern semantics (mirrors Rust L145-179).
// ---------------------------------------------------------------------------

function testFtsSubstringSplitsOnStarAndDropsEmpty() {
  const p = ExplorerFtsPattern.substring(Buffer.from('a*b*'), false);
  assert.equal(p.parts.length, 2);
  assert.ok(p.parts[0].equals(Buffer.from('a')));
  assert.ok(p.parts[1].equals(Buffer.from('b')));
  const p2 = ExplorerFtsPattern.substring(Buffer.from('**hello**world**'), false);
  assert.equal(p2.parts.length, 2);
  assert.ok(p2.parts[0].equals(Buffer.from('hello')));
  assert.ok(p2.parts[1].equals(Buffer.from('world')));
}

function testFtsSubstringMatchesCaseInsensitiveInOrderWithAdvancement() {
  const p = ExplorerFtsPattern.substring(Buffer.from('ERROR*INFO'), false);
  // Case-insensitive ASCII fold.
  assert.equal(p.matches(Buffer.from('error happened, then info later')), true);
  // Out-of-order: "info" before "error" -> no match.
  assert.equal(p.matches(Buffer.from('info before error')), false);
  // No "INFO" anywhere -> no match.
  assert.equal(p.matches(Buffer.from('error happened only')), false);
  // Three-part pattern: each part must follow in order.
  const p2 = ExplorerFtsPattern.substring(Buffer.from('BOOT*KERNEL*SHUTDOWN'), false);
  assert.equal(p2.matches(Buffer.from('system boot kernel panic shutdown')), true);
  // Kernel before boot -> no match.
  assert.equal(p2.matches(Buffer.from('kernel before boot shutdown')), false);
}

function testFtsSubstringEmptyPartsMatchAllEmptyValueMatchesNone() {
  const p = ExplorerFtsPattern.substring(Buffer.from('**'), false);
  assert.equal(p.parts.length, 0);
  // Empty parts => match all.
  assert.equal(p.matches(Buffer.from('anything')), true);
  // Empty value never matches.
  assert.equal(p.matches(Buffer.from('')), false);
}

// ---------------------------------------------------------------------------
// Filter / facet / histogram correctness on a synthetic file (mirrors
// Go TestExplorerTraversalFacetsHistogramFiltersAndRows L11-46).
// ---------------------------------------------------------------------------

function testExplorerTraversalFiltersFacetsHistogramAndRows() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-tfhr-'));
  try {
    const path = join(tempDir, 'simple.journal');
    const entries = [
      [1_000, [['MESSAGE', 'alpha'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [2_000, [['MESSAGE', 'beta'], ['SERVICE', 'api'], ['PRIORITY', '5']]],
      [3_000, [['MESSAGE', 'gamma'], ['SERVICE', 'worker'], ['PRIORITY', '6']]],
      [4_000, [['MESSAGE', 'error alpha'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [5_000, [['MESSAGE', 'debug'], ['SERVICE', 'worker'], ['PRIORITY', '4']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery()
        .withFilter('SERVICE', ['api'])
        .withFacet('PRIORITY')
        .withFacet('SERVICE')
        .withHistogram('PRIORITY')
        .withFtsPattern('alpha');
      q.useSourceRealtime = false;
      q.limit = 10;
      const result = reader.explore(q);
      // Two rows match: SERVICE=api AND FTS contains "alpha"
      // (realtime 1_000 "alpha" and realtime 4_000 "error alpha").
      assert.equal(result.rows.length, 2);
      assert.equal(facetCount(result, fieldHex('PRIORITY'), Buffer.from('6')), 2n);
      assert.equal(facetCount(result, fieldHex('SERVICE'), Buffer.from('api')), 2n);
      assert.equal(histogramTotalForValue(result.histogram, Buffer.from('6')), 2n);
      assert.equal(result.stats.rowsReturned, 2n);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Index strategy matches Traversal on Index-supported shapes (mirrors Go
// TestExplorerIndexStrategyMatchesTraversalForAllValues / Rust L3949).
// ---------------------------------------------------------------------------

function testExplorerIndexStrategyMatchesTraversalForAllValues() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-index-'));
  try {
    const path = join(tempDir, 'index.journal');
    const entries = [
      [1_000, [['MESSAGE', 'one'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [2_000, [['MESSAGE', 'two'], ['SERVICE', 'api'], ['PRIORITY', '5']]],
      [3_000, [['MESSAGE', 'three'], ['SERVICE', 'worker'], ['PRIORITY', '6']]],
      [4_000, [['MESSAGE', 'four'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('PRIORITY').withHistogram('PRIORITY');
      q.useSourceRealtime = false;
      q.fieldMode = ExplorerFieldMode.AllValues;
      q.limit = 2;
      const traversal = reader.exploreWithStrategy(q, ExplorerStrategy.Traversal);
      const indexed = reader.exploreWithStrategy(q, ExplorerStrategy.Index);
      assert.equal(traversal.rows.length, indexed.rows.length);
      for (let i = 0; i < traversal.rows.length; i++) {
        assert.equal(traversal.rows[i].realtimeUsec, indexed.rows[i].realtimeUsec);
        assert.equal(
          traversal.rows[i].payloads.length,
          indexed.rows[i].payloads.length,
        );
        for (let j = 0; j < traversal.rows[i].payloads.length; j++) {
          assert.ok(traversal.rows[i].payloads[j].equals(indexed.rows[i].payloads[j]));
        }
      }
      // Facets must be identical in count. Compare as sorted lists
      // because the Indexed and Traversal strategies may fill Maps
      // in different insertion orders.
      const tFacet = traversal.facets.get(fieldHex('PRIORITY').toString('hex'));
      const iFacet = indexed.facets.get(fieldHex('PRIORITY').toString('hex'));
      const tSorted = [...(tFacet || new Map()).entries()].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      const iSorted = [...(iFacet || new Map()).entries()].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      assert.deepEqual(tSorted, iSorted);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Compare strategy runs both and verifies equality (mirrors Go
// TestExplorerIndexCompareMatchesTraversal L75-105).
// ---------------------------------------------------------------------------

function testExplorerCompareStrategyVerifiesEqualityAndFillsComparison() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-cmp-'));
  try {
    const path = join(tempDir, 'compare.journal');
    const entries = [
      [1_000, [['MESSAGE', 'one'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [2_000, [['MESSAGE', 'two'], ['SERVICE', 'api'], ['PRIORITY', '5']]],
      [3_000, [['MESSAGE', 'three'], ['SERVICE', 'worker'], ['PRIORITY', '6']]],
      [4_000, [['MESSAGE', 'four'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('PRIORITY').withHistogram('PRIORITY');
      q.useSourceRealtime = false;
      q.fieldMode = ExplorerFieldMode.AllValues;
      q.limit = 2;
      const result = reader.exploreWithStrategy(q, ExplorerStrategy.Compare);
      assert.ok(result.comparison instanceof ExplorerComparison);
      assert.ok(result.comparison.traversalDuration >= 0);
      assert.ok(result.comparison.indexDuration >= 0);
      // All 4 entries contribute to the facet count (no filter).
      assert.equal(facetCount(result, fieldHex('PRIORITY'), Buffer.from('6')), 3n);
      assert.equal(facetCount(result, fieldHex('PRIORITY'), Buffer.from('5')), 1n);
      assert.equal(histogramTotalForValue(result.histogram, Buffer.from('6')), 3n);
      assert.equal(histogramTotalForValue(result.histogram, Buffer.from('5')), 1n);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerIndexCompareMatchesTraversalWithFilters() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-cmp-filter-'));
  try {
    const path = join(tempDir, 'compare-filter.journal');
    const entries = [
      [1_000, [['MESSAGE', 'one'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [2_000, [['MESSAGE', 'two'], ['SERVICE', 'api'], ['PRIORITY', '5']]],
      [3_000, [['MESSAGE', 'three'], ['SERVICE', 'worker'], ['PRIORITY', '6']]],
      [4_000, [['MESSAGE', 'four'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery()
        .withFilter('SERVICE', ['api'])
        .withFacet('PRIORITY')
        .withHistogram('PRIORITY');
      q.useSourceRealtime = false;
      q.fieldMode = ExplorerFieldMode.AllValues;
      q.limit = 10;
      const result = reader.exploreWithStrategy(q, ExplorerStrategy.Compare);
      assert.ok(result.comparison instanceof ExplorerComparison);
      assert.equal(result.stats.rowsMatched, 3n);
      assert.equal(facetCount(result, fieldHex('PRIORITY'), Buffer.from('6')), 2n);
      assert.equal(facetCount(result, fieldHex('PRIORITY'), Buffer.from('5')), 1n);
      assert.equal(histogramTotalForValue(result.histogram, Buffer.from('6')), 2n);
      assert.equal(histogramTotalForValue(result.histogram, Buffer.from('5')), 1n);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerIndexCollectRowsDoesNotUseLinearIndexOf() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-indexof-'));
  try {
    const path = join(tempDir, 'indexof.journal');
    writeManyAlternating(path, 64);
    const reader = FileReader.open(path);
    try {
      const originalIndexOf = reader.entryOffsets.indexOf;
      reader.entryOffsets.indexOf = () => {
        throw new Error('entryOffsets.indexOf must not be used in indexed row collection');
      };
      const q = new ExplorerQuery().withFacet('SERVICE').withHistogram('SERVICE');
      q.useSourceRealtime = false;
      q.fieldMode = ExplorerFieldMode.AllValues;
      q.limit = 10;
      try {
        const result = reader.exploreWithStrategy(q, ExplorerStrategy.Index);
        assert.equal(result.rows.length, 10);
      } finally {
        reader.entryOffsets.indexOf = originalIndexOf;
      }
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerSamplingSkipsAndEstimatesRowsBeforeExpansion() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-sampling-'));
  try {
    const path = join(tempDir, 'sampling.journal');
    const count = 600;
    const base = 1_700_000_000_000_000n;
    writeManyAlternating(path, count);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('SERVICE').withHistogram('SERVICE');
      q.useSourceRealtime = false;
      q.limit = 5;
      q.afterRealtimeUsec = base;
      q.beforeRealtimeUsec = base + BigInt(count);
      q.histogramTargetBuckets = 2;
      q.sampling = new ExplorerSampling({
        budget: 20,
        matchedFiles: 1,
        fileHeadRealtimeUsec: base,
        fileTailRealtimeUsec: base + BigInt(count - 1),
        fileEntries: count,
      });
      const result = reader.explore(q);
      assert.ok(result.stats.samplingSampled > 0n);
      assert.ok(result.stats.samplingUnsampled > 0n);
      assert.ok(result.stats.rowsUnsampled > 0n || result.stats.rowsEstimated > 0n);
      assert.ok(
        histogramTotalForValue(result.histogram, Buffer.from('[unsampled]'))
        + histogramTotalForValue(result.histogram, Buffer.from('[estimated]')) > 0n,
      );
      assert.ok(result.stats.rowsExamined < BigInt(count));
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerSamplingSeqnumEstimateClampsProgressAboveOne() {
  const q = new ExplorerQuery().withFacet('SERVICE');
  q.afterRealtimeUsec = 1700000000_000000n;
  q.beforeRealtimeUsec = 1700000001_000000n;
  q.direction = Direction.Backward;
  q.limit = 5;
  q.sampling = new ExplorerSampling({
    budget: 20,
    matchedFiles: 1,
    fileHeadRealtimeUsec: q.afterRealtimeUsec,
    fileTailRealtimeUsec: q.beforeRealtimeUsec,
    fileHeadSeqnum: 1,
    fileTailSeqnum: 100,
    fileEntries: 100,
  });
  const state = _ExplorerSamplingState.forQuery(q, null);
  assert.ok(state !== null);
  state.perFileSampled = 10n;
  assert.equal(state._estimateRemainingRowsBySeqnum(99n), 90n);
}

function testExplorerControlCandidateRowCallbackFeedsSamplingDecision() {
  const q = new ExplorerQuery();
  q.limit = 0;
  const calls = [];
  const control = new ExplorerControl();
  control.setSamplingState({
    decide(commitRealtime, seqnum, candidate) {
      calls.push([commitRealtime, seqnum, candidate]);
      return null;
    },
  });
  control.setCandidateRowCallback((realtimeUsec) => realtimeUsec === 123n);

  assert.equal(_combinedSamplingDecision(q, [], 123n, 7n, null, control), null);
  assert.deepEqual(calls, [[123n, 7n, true]]);
}

function testExplorerIndexRejectsFirstValueSemantics() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-irejfv-'));
  try {
    const path = join(tempDir, 'index-reject.journal');
    writeSimpleEntries(path, [[1_000, [['MESSAGE', 'one'], ['SERVICE', 'api']]]]);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('SERVICE');
      // field_mode is FIRST_VALUE by default.
      assert.throws(
        () => reader.exploreWithStrategy(q, ExplorerStrategy.Index),
        (err) => err instanceof ExplorerUnsupported && /AllValues/.test(err.message),
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerIndexRejectsFts() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-irejfts-'));
  try {
    const path = join(tempDir, 'index-fts.journal');
    writeSimpleEntries(path, [[1_000, [['MESSAGE', 'one'], ['SERVICE', 'api']]]]);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('SERVICE').withFtsPattern('foo');
      q.fieldMode = ExplorerFieldMode.AllValues;
      assert.throws(
        () => reader.exploreWithStrategy(q, ExplorerStrategy.Index),
        (err) => err instanceof ExplorerUnsupported && /FTS/.test(err.message),
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Debug-only column traversal flag is rejected (mirrors Rust L3572-3600).
// ---------------------------------------------------------------------------

function testExplorerRejectsDebugRowTraversalColumnCollection() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-debug-'));
  try {
    const path = join(tempDir, 'debug.journal');
    writeSimpleEntries(path, [[1_000, [['MESSAGE', 'one']]]]);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery();
      q.debugCollectColumnFieldsByRowTraversal = true;
      assert.throws(
        () => reader.explore(q),
        (err) => err instanceof ExplorerUnsupported && /debug/.test(err.message.toLowerCase()),
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Control: progress callback fires, cancellation stops early, deadline stops
// with TIMED_OUT, default progress interval is 250ms (mirrors Rust L3482-3535).
// ---------------------------------------------------------------------------

function testExplorerControlProgressCallbackFiresDuringLargeScan() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-prog-'));
  try {
    const path = join(tempDir, 'progress.journal');
    writeManyAlternating(path, 9_000);
    const reader = FileReader.open(path);
    try {
      const reports = [];
      const ctl = new ExplorerControl();
      ctl.setProgressIntervalMs(0);
      ctl.setProgressCallback((p) => reports.push(Number(p.stats.rowsExamined)));
      const q = new ExplorerQuery();
      q.facets = [Buffer.from('SERVICE')];
      q.limit = 0;
      const result = reader.exploreWithStrategyAndControl(q, ExplorerStrategy.Traversal, ctl);
      assert.equal(ctl.stopReason, null);
      assert.equal(Number(result.stats.rowsExamined), 9_000);
      assert.ok(reports.length > 0, 'progress should fire at least once');
      // 8192-row check step means the last progress emit happens at
      // row >= 8191, so we should see it.
      assert.ok(
        reports.some((r) => r >= Number(EXPLORER_CONTROL_CHECK_EVERY_ROWS) - 1),
        `progress reports missing 8192-row check (saw max=${reports[reports.length - 1]})`,
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerControlCancellationStopsScanWithReason() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-cancel-'));
  try {
    const path = join(tempDir, 'cancel.journal');
    writeManyAlternating(path, 9_000);
    const reader = FileReader.open(path);
    try {
      const ctl = new ExplorerControl();
      ctl.setCancellationCallback(() => true); // cancel immediately
      const q = new ExplorerQuery();
      q.facets = [Buffer.from('SERVICE')];
      q.limit = 0;
      reader.exploreWithStrategyAndControl(q, ExplorerStrategy.Traversal, ctl);
      assert.equal(ctl.stopReason, ExplorerStopReason.Cancelled);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerControlDeadlineStopsScanWithTimedOut() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-deadline-'));
  try {
    const path = join(tempDir, 'deadline.journal');
    writeManyAlternating(path, 9_000);
    const reader = FileReader.open(path);
    try {
      const ctl = new ExplorerControl();
      // Set a deadline that has already passed. The control check
      // fires at most every EXPLORER_CONTROL_CHECK_EVERY_ROWS rows;
      // a near-immediate deadline is the safest cross-platform way to
      // make the scan stop with TIMED_OUT.
      ctl.setDeadline(Date.now() - 1_000);
      const q = new ExplorerQuery();
      q.facets = [Buffer.from('SERVICE')];
      q.limit = 0;
      reader.exploreWithStrategyAndControl(q, ExplorerStrategy.Traversal, ctl);
      assert.equal(ctl.stopReason, ExplorerStopReason.TimedOut);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerControlDefaultProgressIntervalIs250ms() {
  const ctl = new ExplorerControl();
  assert.equal(ctl.progressIntervalMs, EXPLORER_PROGRESS_INTERVAL_MS);
  assert.equal(ctl.progressIntervalMs, 250);
}

// ---------------------------------------------------------------------------
// Stats counters move as expected on a traversal.
// ---------------------------------------------------------------------------

function testExplorerStatsSanityOnTraversal() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-stats-'));
  try {
    const path = join(tempDir, 'stats.journal');
    const entries = [
      [1_000, [['MESSAGE', 'a'], ['SERVICE', 'api']]],
      [2_000, [['MESSAGE', 'b'], ['SERVICE', 'api']]],
      [3_000, [['MESSAGE', 'c'], ['SERVICE', 'worker']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('SERVICE');
      q.useSourceRealtime = false;
      q.limit = 10;
      const result = reader.explore(q);
      assert.equal(Number(result.stats.rowsExamined), 3);
      assert.equal(Number(result.stats.rowsMatched), 3);
      assert.equal(Number(result.stats.rowsReturned), 3);
      assert.equal(Number(result.stats.facetRowsMatched), 3);
      assert.ok(Number(result.stats.facetUpdates) >= 2);
      assert.equal(Number(result.stats.lastRealtimeUsec), 3_000);
      assert.ok(Number(result.stats.dataRefsSeen) >= 6);
      assert.ok(Number(result.stats.dataObjectsClassified) >= 3);
      // All 24 counter fields exist and are BigInts.
      const counters = [
        'rowsExamined', 'rowsMatched', 'facetRowsMatched',
        'rowsReturned', 'rowsUnsampled', 'rowsEstimated',
        'samplingSampled', 'samplingUnsampled', 'samplingEstimated',
        'lastRealtimeUsec', 'maxSourceRealtimeDeltaUsec',
        'dataRefsSeen', 'dataRefsSkipped', 'dataPayloadsLoaded',
        'dataObjectsClassified', 'dataCacheHits', 'dataCacheMisses',
        'payloadsDecompressed', 'ftsScans', 'facetUpdates',
        'histogramUpdates', 'returnedRowExpansions',
        'earlyStopOpportunities', 'earlyStops',
      ];
      for (const f of counters) {
        assert.ok(result.stats[f] !== undefined, `missing counter: ${f}`);
        assert.equal(typeof result.stats[f], 'bigint', `counter ${f} should be bigint`);
      }
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// columnFields comes from the FIELD hash-table index, not row traversal,
// and is suppressed when the debug flag is enabled (rejected outright).
// ---------------------------------------------------------------------------

function testExplorerColumnFieldsComeFromFieldIndex() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-cols-'));
  try {
    const path = join(tempDir, 'columns.journal');
    const entries = [
      [1_000, [['MESSAGE', 'a'], ['SERVICE', 'api'], ['PRIORITY', '6']]],
      [2_000, [['MESSAGE', 'b'], ['SERVICE', 'api'], ['PRIORITY', '5']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFacet('SERVICE');
      const result = reader.explore(q);
      const expected = new Set(['MESSAGE', 'SERVICE', 'PRIORITY']);
      for (const e of expected) {
        assert.ok(result.columnFields.has(e), `columnFields missing ${e}`);
      }
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Field-mode: FirstValue counts one value per selected field, AllValues
// counts duplicates (mirrors Rust L4060-4104).
// ---------------------------------------------------------------------------

function testExplorerFirstValueCountsOneValuePerField() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-fm-'));
  try {
    const path = join(tempDir, 'first.journal');
    const entries = [
      [1_000, [['MESSAGE', 'one'], ['TAG', 'a'], ['TAG', 'b']]],
      [2_000, [['MESSAGE', 'two'], ['TAG', 'b']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const first = new ExplorerQuery().withFacet('TAG');
      first.limit = 0;
      first.useSourceRealtime = false;
      const firstResult = reader.explore(first);
      // FirstValue: each row contributes at most one TAG value
      // (the first one seen). Row 1 -> 'a'; Row 2 -> 'b'.
      assert.equal(facetCount(firstResult, fieldHex('TAG'), Buffer.from('a')), 1n);
      assert.equal(facetCount(firstResult, fieldHex('TAG'), Buffer.from('b')), 1n);

      const allValues = new ExplorerQuery().withFacet('TAG');
      allValues.limit = 0;
      allValues.useSourceRealtime = false;
      allValues.fieldMode = ExplorerFieldMode.AllValues;
      const allResult = reader.explore(allValues);
      // AllValues: row 1 contributes both 'a' and 'b'; row 2
      // contributes 'b'. So 'a' = 1, 'b' = 2.
      assert.equal(facetCount(allResult, fieldHex('TAG'), Buffer.from('a')), 1n);
      assert.equal(facetCount(allResult, fieldHex('TAG'), Buffer.from('b')), 2n);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Stop-when-rows-full + stop_when_rows_full_check_every. Mirrors Rust
// should_stop_when_rows_full L3005-3036.
// ---------------------------------------------------------------------------

function testExplorerStopWhenRowsFullTruncatesScan() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-stop-'));
  try {
    const path = join(tempDir, 'stop.journal');
    const entries = [
      [1_000, [['MESSAGE', 'a'], ['SERVICE', 'api']]],
      [2_000, [['MESSAGE', 'b'], ['SERVICE', 'api']]],
      [3_000, [['MESSAGE', 'c'], ['SERVICE', 'api']]],
      [4_000, [['MESSAGE', 'd'], ['SERVICE', 'api']]],
      [5_000, [['MESSAGE', 'e'], ['SERVICE', 'api']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      const q = new ExplorerQuery().withFilter('SERVICE', ['api']);
      q.useSourceRealtime = false;
      q.limit = 2;
      q.stopWhenRowsFull = true;
      // Use a tiny slack window so the stop fires as soon as the
      // commit realtime passes the newest row plus the slack.
      q.realtimeSlackUsec = 0n;
      const result = reader.explore(q);
      // The scan should terminate before exhausting all 5 entries
      // because we already have 2 rows and the commit realtime
      // grows past newest + slack.
      assert.ok(Number(result.stats.rowsExamined) < 5, `rowsExamined=${Number(result.stats.rowsExamined)}`);
      assert.ok(result.rows.length <= 2, `rows.length=${result.rows.length}`);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerTraversalKeepsZeroCommitRealtimeRows() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-zero-'));
  try {
    const path = join(tempDir, 'zero.journal');
    writeSimpleEntries(path, [
      [0, [['MESSAGE', 'zero']]],
      [1_000, [['MESSAGE', 'one']]],
    ]);
    const reader = FileReader.open(path);
    try {
      const result = reader.explore(new ExplorerQuery());
      assert.equal(result.rows.length, 2);
      assert.ok(result.rows.some(row => row.realtimeUsec === 0n));
      assert.equal(result.stats.rowsMatched, 2n);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// FTS terms: positive and negative patterns filter rows.
// ---------------------------------------------------------------------------

function testExplorerFtsPositiveAndNegativePatternsFilterRows() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-fts-'));
  try {
    const path = join(tempDir, 'fts.journal');
    const entries = [
      [1_000, [['MESSAGE', 'normal alpha info']]],
      [2_000, [['MESSAGE', 'normal beta info']]],
      [3_000, [['MESSAGE', 'normal gamma']]],
    ];
    writeSimpleEntries(path, entries);
    const reader = FileReader.open(path);
    try {
      // Use the raw fts_patterns / fts_negative_patterns lists
      // (the parallel-bytes form) so positive and negative
      // patterns are evaluated independently. The
      // withFtsPattern / withFtsNegativePattern builders mirror
      // the Rust behavior where the fts_terms list returns on the
      // first matching term; mixing them in the same query honors
      // first-match ordering. The raw lists path matches the Rust
      // `matches_fts` helper for both positive and negative axes.
      const q = new ExplorerQuery();
      q.ftsPatterns = [Buffer.from('info')];
      q.ftsNegativePatterns = [Buffer.from('beta')];
      q.useSourceRealtime = false;
      const result = reader.explore(q);
      // Row 1 has "info" but not "beta" -> match.
      // Row 2 has "info" and "beta" -> reject.
      // Row 3 has neither -> reject.
      assert.equal(result.rows.length, 1);
      assert.equal(Number(result.rows[0].realtimeUsec), 1_000);
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Query validation: invalid time window raises ExplorerError.
// ---------------------------------------------------------------------------

function testExplorerQueryValidationRejectsInvertedTimeWindow() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-vtw-'));
  try {
    const path = join(tempDir, 'v.journal');
    writeSimpleEntries(path, [[1_000, [['MESSAGE', 'a']]]]);
    const q = new ExplorerQuery();
    q.afterRealtimeUsec = 2_000n;
    q.beforeRealtimeUsec = 1_000n;
    const reader = FileReader.open(path);
    try {
      assert.throws(
        () => reader.explore(q),
        (err) => err instanceof ExplorerError && /after_realtime_usec/.test(err.message),
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerQueryValidationRejectsDuplicateFacets() {
  const tempDir = mkdtempSync(join(tmpdir(), 'node-explor-dup-'));
  try {
    const path = join(tempDir, 'dup.journal');
    writeSimpleEntries(path, [[1_000, [['SERVICE', 'api']]]]);
    const q = new ExplorerQuery().withFacet('SERVICE').withFacet('SERVICE');
    const reader = FileReader.open(path);
    try {
      assert.throws(
        () => reader.explore(q),
        (err) => err instanceof ExplorerError && /duplicate/.test(err.message.toLowerCase()),
      );
    } finally {
      reader.close();
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function testExplorerResultClassShape() {
  // Sanity: the result has the documented fields/types.
  const r = new ExplorerResult();
  assert.ok(Array.isArray(r.rows));
  assert.ok(r.facets instanceof Map);
  assert.equal(r.histogram, null);
  assert.ok(r.columnFields instanceof Set);
  assert.ok(r.stats instanceof ExplorerStats);
  assert.equal(r.comparison, null);
}

function testExplorerStatsSnakeCaseJsonKeys() {
  const s = new ExplorerStats();
  const json = s.toJson();
  const expected = [
    'rows_examined', 'rows_matched', 'facet_rows_matched',
    'rows_returned', 'rows_unsampled', 'rows_estimated',
    'sampling_sampled', 'sampling_unsampled', 'sampling_estimated',
    'last_realtime_usec', 'max_source_realtime_delta_usec',
    'data_refs_seen', 'data_refs_skipped', 'data_payloads_loaded',
    'data_objects_classified', 'data_cache_hits', 'data_cache_misses',
    'payloads_decompressed', 'fts_scans', 'facet_updates',
    'histogram_updates', 'returned_row_expansions',
    'early_stop_opportunities', 'early_stops',
  ];
  for (const k of expected) {
    assert.ok(Object.prototype.hasOwnProperty.call(json, k), `missing snake_case key: ${k}`);
    assert.equal(typeof json[k], 'number', `snake_case ${k} should serialize as number`);
  }
}

function testUnsetValueConstantExported() {
  assert.ok(UNSET_VALUE instanceof Buffer);
  assert.equal(UNSET_VALUE.toString('utf8'), '-');
}

function testExplorerAnchorFactories() {
  assert.equal(ExplorerAnchor.auto().kind, ExplorerAnchorKind.Auto);
  assert.equal(ExplorerAnchor.head().kind, ExplorerAnchorKind.Head);
  assert.equal(ExplorerAnchor.tail().kind, ExplorerAnchorKind.Tail);
  assert.equal(ExplorerAnchor.realtime(123n).kind, ExplorerAnchorKind.Realtime);
  assert.equal(ExplorerAnchor.realtime(123n).realtimeUsec, 123n);
}

function testExplorerSamplingSentinels() {
  const s = new ExplorerSampling();
  assert.equal(s.budget, 0n);
  assert.equal(s.matchedFiles, 0n);
  assert.equal(s.fileHeadRealtimeUsec, 0n);
  assert.equal(s.fileTailRealtimeUsec, 0n);
  assert.equal(s.fileHeadSeqnum, 0n);
  assert.equal(s.fileTailSeqnum, 0n);
  assert.equal(s.fileEntries, 0n);
}

// ---------------------------------------------------------------------------
// Run.
// ---------------------------------------------------------------------------

export async function run() {
  testExplorerQueryDefaultsMatchRust();
  testExplorerQueryBuildersReturnSelf();
  testFtsSubstringSplitsOnStarAndDropsEmpty();
  testFtsSubstringMatchesCaseInsensitiveInOrderWithAdvancement();
  testFtsSubstringEmptyPartsMatchAllEmptyValueMatchesNone();
  testExplorerTraversalFiltersFacetsHistogramAndRows();
  testExplorerIndexStrategyMatchesTraversalForAllValues();
  testExplorerCompareStrategyVerifiesEqualityAndFillsComparison();
  testExplorerIndexCompareMatchesTraversalWithFilters();
  testExplorerIndexCollectRowsDoesNotUseLinearIndexOf();
  testExplorerSamplingSkipsAndEstimatesRowsBeforeExpansion();
  testExplorerSamplingSeqnumEstimateClampsProgressAboveOne();
  testExplorerControlCandidateRowCallbackFeedsSamplingDecision();
  testExplorerIndexRejectsFirstValueSemantics();
  testExplorerIndexRejectsFts();
  testExplorerRejectsDebugRowTraversalColumnCollection();
  testExplorerControlProgressCallbackFiresDuringLargeScan();
  testExplorerControlCancellationStopsScanWithReason();
  testExplorerControlDeadlineStopsScanWithTimedOut();
  testExplorerControlDefaultProgressIntervalIs250ms();
  testExplorerStatsSanityOnTraversal();
  testExplorerColumnFieldsComeFromFieldIndex();
  testExplorerFirstValueCountsOneValuePerField();
  testExplorerStopWhenRowsFullTruncatesScan();
  testExplorerTraversalKeepsZeroCommitRealtimeRows();
  testExplorerFtsPositiveAndNegativePatternsFilterRows();
  testExplorerQueryValidationRejectsInvertedTimeWindow();
  testExplorerQueryValidationRejectsDuplicateFacets();
  testExplorerResultClassShape();
  testExplorerStatsSnakeCaseJsonKeys();
  testUnsetValueConstantExported();
  testExplorerAnchorFactories();
  testExplorerSamplingSentinels();
}
