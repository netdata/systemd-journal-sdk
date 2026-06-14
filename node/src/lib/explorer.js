// Pure-JS journal explorer.
//
// Mirrors the semantics of rust/src/journal/src/explorer.rs (the source
// of truth) for the public surface listed in SOW-0105. The closest
// architectural precedent is python/journal/explorer.py (SOW-0104):
// both Python and Node lack mmap, so the reader iteration is
// entry-array-based with positioned Buffer reads. Rust wins on any
// semantic conflict; Python's porting decisions are mirrored only
// where they agree with Rust.
//
// Module-private helpers below the public classes are kept in the
// same file because the Node reader is single-purpose; multi-file
// explore stays module-private (no DirectoryExplorer). Field
// enumeration reuses FileReader._enumerateFieldsIndexed (the FIELD
// hash-table path), so the column catalog comes from the index, not
// from row traversal.

import { Buffer } from 'node:buffer';
import { HASH_ITEM_SIZE, OBJECT_HEADER_SIZE } from './header.js';

export const DEFAULT_HISTOGRAM_TARGET_BUCKETS = 150;
export const DEFAULT_TIME_SLACK_USEC = 120_000_000n;
export const EXPLORER_CONTROL_CHECK_EVERY_ROWS = 8192n;
export const DEFAULT_ROWS_FULL_CHECK_EVERY_ROWS = 1n;
export const EXPLORER_PROGRESS_INTERVAL_MS = 250;
export const EXPLORER_SAMPLING_SLOTS_MAX = 1000;
export const EXPLORER_SAMPLING_RECALIBRATE_ROWS = 10_000n;
export const EXPLORER_SAMPLING_ESTIMATE_AFTER_PROGRESS = 0.01;
export const EXPLORER_HISTOGRAM_MAX_BUCKETS = 1001;
export const EXPLORER_HISTOGRAM_DEFAULT_WINDOW_USEC = 3_600_000_000n;

const SOURCE_REALTIME_FIELD = Buffer.from('_SOURCE_REALTIME_TIMESTAMP');
export const UNSET_VALUE = Buffer.from('-');
const EXPLORER_UNSAMPLED_VALUE = Buffer.from('[unsampled]');
const EXPLORER_ESTIMATED_VALUE = Buffer.from('[estimated]');

const FACET_PUBLIC = 0x01;
const FACET_HISTOGRAM = 0x02;
const FACET_SOURCE_REALTIME = 0x04;

const _OFFSET_CLASS_IRRELEVANT = 1;
const _OFFSET_CLASS_FTS_MATCH = 2;
const _OFFSET_CLASS_FTS_NEGATIVE = 3;
const _OFFSET_CLASS_VALUE_BASE = 4;

const _VALID_HISTOGRAM_BAR_SECONDS = [
  1, 2, 5, 10, 15, 30, 60, 120, 180, 300, 600, 900,
  1800, 3600, 7200, 21600, 28800, 43200, 86400,
  172800, 259200, 432000, 604800, 1209600, 2592000,
];

export class ExplorerError extends Error {}
export class ExplorerUnsupported extends ExplorerError {}

export const Direction = Object.freeze({ Forward: 0, Backward: 1 });

export const ExplorerAnchorKind = Object.freeze({
  Auto: 'auto',
  Head: 'head',
  Tail: 'tail',
  Realtime: 'realtime',
});

export class ExplorerAnchor {
  constructor(kind = ExplorerAnchorKind.Auto, realtimeUsec = 0n) {
    this.kind = kind;
    this.realtimeUsec = BigInt(realtimeUsec);
  }
  static auto() { return new ExplorerAnchor(ExplorerAnchorKind.Auto, 0n); }
  static head() { return new ExplorerAnchor(ExplorerAnchorKind.Head, 0n); }
  static tail() { return new ExplorerAnchor(ExplorerAnchorKind.Tail, 0n); }
  static realtime(usec) { return new ExplorerAnchor(ExplorerAnchorKind.Realtime, BigInt(usec)); }
}

export const ExplorerFieldMode = Object.freeze({ AllValues: 'all_values', FirstValue: 'first_value' });
export const ExplorerStrategy = Object.freeze({ Traversal: 'traversal', Index: 'index', Compare: 'compare' });
export const ExplorerStopReason = Object.freeze({ TimedOut: 'timed_out', Cancelled: 'cancelled' });

export class ExplorerFilter {
  constructor(field, values = []) {
    this.field = _toBytes(field);
    this.values = values.map(_toBytes);
  }
  static new(field, values) {
    return new ExplorerFilter(field, values);
  }
}

export class ExplorerFtsPattern {
  constructor(parts = [], negative = false) {
    this.parts = parts;
    this.negative = Boolean(negative);
  }
  static substring(pattern, negative = false) {
    const buf = _toBytes(pattern);
    const parts = [];
    for (const part of buf.toString('latin1').split('*')) {
      if (part.length === 0) continue;
      parts.push(Buffer.from(part, 'latin1'));
    }
    return new ExplorerFtsPattern(parts, Boolean(negative));
  }
  matches(value) {
    let v = _toBytes(value);
    if (v.length === 0) return false;
    if (this.parts.length === 0) return true;
    let offset = 0;
    for (const part of this.parts) {
      const idx = _findAsciiCaseInsensitive(v, part, offset);
      if (idx === -1) return false;
      offset = idx + part.length;
    }
    return true;
  }
}

export class ExplorerSampling {
  constructor(init = {}) {
    this.budget = BigInt(init.budget ?? 0);
    this.matchedFiles = BigInt(init.matchedFiles ?? init.matched_files ?? 0);
    this.fileHeadRealtimeUsec = BigInt(init.fileHeadRealtimeUsec ?? init.file_head_realtime_usec ?? 0);
    this.fileTailRealtimeUsec = BigInt(init.fileTailRealtimeUsec ?? init.file_tail_realtime_usec ?? 0);
    this.fileHeadSeqnum = BigInt(init.fileHeadSeqnum ?? init.file_head_seqnum ?? 0);
    this.fileTailSeqnum = BigInt(init.fileTailSeqnum ?? init.file_tail_seqnum ?? 0);
    this.fileEntries = BigInt(init.fileEntries ?? init.file_entries ?? 0);
  }
}

export class ExplorerStats {
  constructor() {
    this.rowsExamined = 0n;
    this.rowsMatched = 0n;
    this.facetRowsMatched = 0n;
    this.rowsReturned = 0n;
    this.rowsUnsampled = 0n;
    this.rowsEstimated = 0n;
    this.samplingSampled = 0n;
    this.samplingUnsampled = 0n;
    this.samplingEstimated = 0n;
    this.lastRealtimeUsec = 0n;
    this.maxSourceRealtimeDeltaUsec = 0n;
    this.dataRefsSeen = 0n;
    this.dataRefsSkipped = 0n;
    this.dataPayloadsLoaded = 0n;
    this.dataObjectsClassified = 0n;
    this.dataCacheHits = 0n;
    this.dataCacheMisses = 0n;
    this.payloadsDecompressed = 0n;
    this.ftsScans = 0n;
    this.facetUpdates = 0n;
    this.histogramUpdates = 0n;
    this.returnedRowExpansions = 0n;
    this.earlyStopOpportunities = 0n;
    this.earlyStops = 0n;
  }
  copy() {
    const out = new ExplorerStats();
    for (const k of Object.keys(this)) out[k] = this[k];
    return out;
  }
  // snake_case names for cross-language stable serialization (matches
  // the Rust serde Serialize output documented in explorer.rs:218-244).
  toJson() {
    return {
      rows_examined: Number(this.rowsExamined),
      rows_matched: Number(this.rowsMatched),
      facet_rows_matched: Number(this.facetRowsMatched),
      rows_returned: Number(this.rowsReturned),
      rows_unsampled: Number(this.rowsUnsampled),
      rows_estimated: Number(this.rowsEstimated),
      sampling_sampled: Number(this.samplingSampled),
      sampling_unsampled: Number(this.samplingUnsampled),
      sampling_estimated: Number(this.samplingEstimated),
      last_realtime_usec: Number(this.lastRealtimeUsec),
      max_source_realtime_delta_usec: Number(this.maxSourceRealtimeDeltaUsec),
      data_refs_seen: Number(this.dataRefsSeen),
      data_refs_skipped: Number(this.dataRefsSkipped),
      data_payloads_loaded: Number(this.dataPayloadsLoaded),
      data_objects_classified: Number(this.dataObjectsClassified),
      data_cache_hits: Number(this.dataCacheHits),
      data_cache_misses: Number(this.dataCacheMisses),
      payloads_decompressed: Number(this.payloadsDecompressed),
      fts_scans: Number(this.ftsScans),
      facet_updates: Number(this.facetUpdates),
      histogram_updates: Number(this.histogramUpdates),
      returned_row_expansions: Number(this.returnedRowExpansions),
      early_stop_opportunities: Number(this.earlyStopOpportunities),
      early_stops: Number(this.earlyStops),
    };
  }
}

export class ExplorerRow {
  constructor(realtimeUsec, cursor, payloads = []) {
    this.realtimeUsec = BigInt(realtimeUsec);
    this.cursor = String(cursor);
    this.payloads = payloads;
  }
}

export class ExplorerHistogramBucket {
  constructor(startUsec, endUsec) {
    this.startRealtimeUsec = BigInt(startUsec);
    this.endRealtimeUsec = BigInt(endUsec);
    this.values = new Map();
  }
}

export class ExplorerHistogram {
  constructor(field, buckets = []) {
    this.field = _toBytes(field);
    this.buckets = buckets;
  }
}

export class ExplorerComparison {
  constructor() {
    this.traversalDuration = 0;
    this.indexDuration = 0;
    this.traversalStats = new ExplorerStats();
    this.indexStats = new ExplorerStats();
  }
}

export class ExplorerResult {
  constructor() {
    this.rows = [];
    this.facets = new Map();
    this.histogram = null;
    this.columnFields = new Set();
    this.stats = new ExplorerStats();
    this.comparison = null;
  }
}

export class ExplorerProgress {
  constructor(stats, elapsed) {
    this.stats = stats.copy();
    this.elapsed = Number(elapsed);
  }
}

export class ExplorerQuery {
  constructor() {
    this.afterRealtimeUsec = null;
    this.beforeRealtimeUsec = null;
    this.anchor = ExplorerAnchor.auto();
    this.direction = Direction.Forward;
    this.limit = 200;
    this.filters = [];
    this.facets = [];
    this.histogram = null;
    this.histogramAfterRealtimeUsec = null;
    this.histogramBeforeRealtimeUsec = null;
    this.histogramTargetBuckets = DEFAULT_HISTOGRAM_TARGET_BUCKETS;
    this.ftsTerms = [];
    this.ftsPatterns = [];
    this.ftsNegativePatterns = [];
    this.fieldMode = ExplorerFieldMode.FirstValue;
    this.excludeFacetFieldFilters = true;
    this.useSourceRealtime = true;
    this.realtimeSlackUsec = DEFAULT_TIME_SLACK_USEC;
    this.stopWhenRowsFull = false;
    this.stopWhenRowsFullCheckEvery = Number(DEFAULT_ROWS_FULL_CHECK_EVERY_ROWS);
    this.sampling = null;
    this.debugCollectColumnFieldsByRowTraversal = false;
  }
  withFilter(field, values) {
    this.filters.push(ExplorerFilter.new(field, values));
    return this;
  }
  withFacet(field) {
    this.facets.push(_toBytes(field));
    return this;
  }
  withHistogram(field) {
    this.histogram = _toBytes(field);
    return this;
  }
  withFtsPattern(pattern) {
    const buf = _toBytes(pattern);
    this.ftsTerms.push(ExplorerFtsPattern.substring(buf, false));
    this.ftsPatterns.push(buf);
    return this;
  }
  withFtsNegativePattern(pattern) {
    const buf = _toBytes(pattern);
    this.ftsTerms.push(ExplorerFtsPattern.substring(buf, true));
    this.ftsNegativePatterns.push(buf);
    return this;
  }
}

export class ExplorerControl {
  constructor() {
    this.deadline = null;
    this.cancellation = null;
    this.progress = null;
    this.matchedRow = null;
    this.sampling = null;
    this.progressIntervalMs = EXPLORER_PROGRESS_INTERVAL_MS;
    this.stopReason = null;
    this._started = Date.now();
    this._lastProgress = Date.now();
    this._nextCheckRows = EXPLORER_CONTROL_CHECK_EVERY_ROWS;
    this._stopped = false;
  }
  setDeadline(deadline) { this.deadline = deadline; }
  setCancellationCallback(cb) { this.cancellation = cb; }
  setProgressCallback(cb) { this.progress = cb; }
  setMatchedRowCallback(cb) { this.matchedRow = cb; }
  setSamplingState(sampling) { this.sampling = sampling; }
  setProgressIntervalMs(ms) { this.progressIntervalMs = Number(ms); }
  shouldStopAfterRows(rowsSeen, stats) {
    if (this._stopped) return true;
    if (rowsSeen < this._nextCheckRows) return false;
    this._nextCheckRows = rowsSeen + EXPLORER_CONTROL_CHECK_EVERY_ROWS;
    return this._check(stats);
  }
  _check(stats) {
    const now = Date.now();
    if (this.progress !== null && (now - this._lastProgress) >= this.progressIntervalMs) {
      this._emitProgress(stats, now);
    }
    if (this.cancellation !== null && this.cancellation()) {
      this.stopReason = ExplorerStopReason.Cancelled;
      this._emitProgress(stats, now);
      this._stopped = true;
      return true;
    }
    if (this.deadline !== null && now >= this.deadline) {
      this.stopReason = ExplorerStopReason.TimedOut;
      this._emitProgress(stats, now);
      this._stopped = true;
      return true;
    }
    return false;
  }
  _emitProgress(stats, now) {
    this._lastProgress = now;
    if (this.progress !== null) {
      this.progress(new ExplorerProgress(stats, (now - this._started) / 1000));
    }
  }
  emitMatchedRow(realtimeUsec, rowsMatched) {
    if (this.matchedRow === null) return false;
    return Boolean(this.matchedRow(realtimeUsec, rowsMatched));
  }
}

export class _ExplorerSamplingState {
  constructor(query, histogramBucketCount) {
    const sampling = query.sampling;
    if (
      sampling === null
      || sampling.budget === 0n
      || sampling.matchedFiles === 0n
      || query.afterRealtimeUsec === null
      || query.beforeRealtimeUsec === null
      || query.afterRealtimeUsec >= query.beforeRealtimeUsec
    ) {
      throw new Error('inactive sampling');
    }

    let slots = histogramBucketCount ?? query.histogramTargetBuckets;
    slots = Math.max(2, Math.min(EXPLORER_SAMPLING_SLOTS_MAX, Number(slots)));
    const delta = query.beforeRealtimeUsec - query.afterRealtimeUsec;
    this.startRealtimeUsec = query.afterRealtimeUsec;
    this.endRealtimeUsec = query.beforeRealtimeUsec;
    this.stepRealtimeUsec = ((delta / BigInt(slots)) - 1n) > 1n
      ? (delta / BigInt(slots)) - 1n
      : 1n;
    this.enableAfterSamples = sampling.budget / 2n;
    this.perFileEnableAfterSamples = _maxBigInt(
      BigInt(query.limit),
      (sampling.budget / 4n) / _maxBigInt(1n, sampling.matchedFiles),
    );
    this.perSlotEnableAfterSamples = _maxBigInt(
      BigInt(query.limit),
      (sampling.budget / 4n) / BigInt(slots),
    );
    this.perSlotSampled = Array.from({ length: slots }, () => 0n);
    this.perSlotUnsampled = Array.from({ length: slots }, () => 0n);
    this.matchedFiles = _maxBigInt(1n, sampling.matchedFiles);
    this.direction = query.direction;
    this.sampled = 0n;
    this.beginFile(sampling);
  }

  static forQuery(query, histogramBucketCount) {
    try {
      return new _ExplorerSamplingState(query, histogramBucketCount);
    } catch {
      return null;
    }
  }

  beginFile(sampling) {
    this.fileHeadRealtimeUsec = sampling.fileHeadRealtimeUsec;
    this.fileTailRealtimeUsec = sampling.fileTailRealtimeUsec;
    this.fileHeadSeqnum = sampling.fileHeadSeqnum;
    this.fileTailSeqnum = sampling.fileTailSeqnum;
    this.fileEntries = sampling.fileEntries;
    this.firstRealtimeUsec = null;
    this.perFileSampled = 0n;
    this.perFileUnsampled = 0n;
    this.perFileEvery = 0n;
    this.perFileSkipped = 0n;
    this.perFileRecalibrate = 0n;
  }

  decide(realtimeUsec, seqnum, candidateToKeep) {
    realtimeUsec = BigInt(realtimeUsec);
    seqnum = BigInt(seqnum ?? 0);
    if (this.firstRealtimeUsec === null) this.firstRealtimeUsec = realtimeUsec;
    if (candidateToKeep) return { action: 'full', sampled: false };

    const slot = this._slotForRealtime(realtimeUsec);
    let shouldSample;
    if (
      this.sampled < this.enableAfterSamples
      || this.perFileSampled < this.perFileEnableAfterSamples
      || this.perSlotSampled[slot] < this.perSlotEnableAfterSamples
    ) {
      shouldSample = true;
    } else if (
      this.perFileRecalibrate >= EXPLORER_SAMPLING_RECALIBRATE_ROWS
      || this.perFileEvery === 0n
    ) {
      this._recalibrate(realtimeUsec, seqnum);
      shouldSample = true;
    } else if (this.perFileSkipped >= this.perFileEvery) {
      this.perFileSkipped = 0n;
      shouldSample = true;
    } else {
      this.perFileSkipped += 1n;
      shouldSample = false;
    }

    if (shouldSample) {
      this.sampled += 1n;
      this.perFileSampled += 1n;
      this.perSlotSampled[slot] += 1n;
      return { action: 'full', sampled: true };
    }

    this.perFileRecalibrate += 1n;
    this.perFileUnsampled += 1n;
    this.perSlotUnsampled[slot] += 1n;

    if (
      this.perFileUnsampled > this.perFileSampled
      && this._progressByTime(realtimeUsec) > EXPLORER_SAMPLING_ESTIMATE_AFTER_PROGRESS
    ) {
      const remainingRows = this._estimateRemainingRows(realtimeUsec, seqnum);
      const [fromRealtimeUsec, toRealtimeUsec] = this._remainingRange(realtimeUsec);
      return {
        action: 'stop',
        remainingRows,
        fromRealtimeUsec,
        toRealtimeUsec,
      };
    }
    return { action: 'skip' };
  }

  _slotForRealtime(realtimeUsec) {
    const clamped = _minBigInt(_maxBigInt(realtimeUsec, this.startRealtimeUsec), this.endRealtimeUsec);
    const slot = Number((clamped - this.startRealtimeUsec) / this.stepRealtimeUsec);
    return Math.min(slot, Math.max(0, this.perSlotSampled.length - 1));
  }

  _recalibrate(realtimeUsec, seqnum) {
    const remaining = this._estimateRemainingRows(realtimeUsec, seqnum);
    const wanted = _maxBigInt(1n, this.enableAfterSamples / this.matchedFiles);
    this.perFileEvery = _maxBigInt(1n, remaining / wanted);
    this.perFileRecalibrate = 0n;
  }

  _estimateRemainingRows(realtimeUsec, seqnum) {
    const bySeqnum = this._estimateRemainingRowsBySeqnum(seqnum);
    if (bySeqnum !== null) return bySeqnum;
    return this._estimateRemainingRowsByTime(realtimeUsec);
  }

  _estimateRemainingRowsBySeqnum(seqnum) {
    if (
      this.fileEntries === 0n
      || this.fileHeadSeqnum === 0n
      || this.fileTailSeqnum === 0n
      || seqnum === 0n
    ) return null;
    const scanned = this._scannedFileRows();
    const span = this.direction === Direction.Forward
      ? _saturatingSub(seqnum, this.fileHeadSeqnum)
      : _saturatingSub(this.fileTailSeqnum, seqnum);
    if (span === 0n) return null;
    let proportion = Number(scanned) / Number(span);
    if (proportion <= 0.0 || !Number.isFinite(proportion)) return null;
    proportion = Math.min(proportion, 1.0);
    const expected = BigInt(Math.trunc(proportion * Number(this.fileEntries)));
    if (expected === 0n) return null;
    return _maxBigInt(1n, _saturatingSub(expected, scanned));
  }

  _estimateRemainingRowsByTime(realtimeUsec) {
    const scanned = this._scannedFileRows();
    const [after, before] = this._overlappingTimeframe(realtimeUsec);
    const [total, remaining] = this._remainingTimeDetails(realtimeUsec, after, before);
    const elapsed = _maxBigInt(1n, _saturatingSub(total, remaining));
    let proportion = Number(elapsed) / Number(_maxBigInt(1n, total));
    if (proportion === 0.0 || proportion > 1.0 || !Number.isFinite(proportion)) proportion = 1.0;
    let expectedTotal = BigInt(Math.trunc(Number(scanned) / proportion));
    if (this.fileEntries !== 0n && expectedTotal > this.fileEntries) expectedTotal = this.fileEntries;
    return _maxBigInt(1n, _saturatingSub(expectedTotal, scanned));
  }

  _scannedFileRows() {
    return _maxBigInt(1n, this.perFileSampled + this.perFileUnsampled);
  }

  _progressByTime(realtimeUsec) {
    const [after, before] = this._overlappingTimeframe(realtimeUsec);
    const total = _maxBigInt(1n, before - after);
    const elapsed = this.direction === Direction.Forward
      ? _saturatingSub(realtimeUsec, after)
      : _saturatingSub(before, realtimeUsec);
    return Number(_minBigInt(elapsed, total)) / Number(total);
  }

  _overlappingTimeframe(realtimeUsec) {
    if (this.direction === Direction.Forward) {
      let oldest = this.firstRealtimeUsec ?? (this.fileHeadRealtimeUsec !== 0n ? this.fileHeadRealtimeUsec : this.startRealtimeUsec);
      let newest = this.fileTailRealtimeUsec !== 0n
        ? _minBigInt(this.endRealtimeUsec, this.fileTailRealtimeUsec)
        : this.endRealtimeUsec;
      if (newest <= oldest) newest = oldest + 1n;
      if (realtimeUsec < oldest) oldest = _saturatingSub(realtimeUsec, 1n);
      return [oldest, newest];
    }
    let newest = this.firstRealtimeUsec ?? (this.fileTailRealtimeUsec !== 0n ? this.fileTailRealtimeUsec : this.endRealtimeUsec);
    const oldest = this.fileHeadRealtimeUsec !== 0n
      ? _maxBigInt(this.startRealtimeUsec, this.fileHeadRealtimeUsec)
      : this.startRealtimeUsec;
    if (newest <= oldest) newest = oldest + 1n;
    if (newest < realtimeUsec) newest = realtimeUsec + 1n;
    return [oldest, newest];
  }

  _remainingRange(realtimeUsec) {
    const [after, before] = this._overlappingTimeframe(realtimeUsec);
    const [, , start, end] = this._remainingTimeDetails(realtimeUsec, after, before);
    return [start, end];
  }

  _remainingTimeDetails(realtimeUsec, after, before) {
    if (realtimeUsec <= after) after = _saturatingSub(realtimeUsec, 1n);
    if (realtimeUsec >= before) before = realtimeUsec + 1n;
    if (before <= after) before = after + 1n;
    const remainingStart = this.direction === Direction.Forward ? realtimeUsec : after;
    const remainingEnd = this.direction === Direction.Forward ? before : realtimeUsec;
    return [
      _maxBigInt(1n, before - after),
      _saturatingSub(remainingEnd, remainingStart),
      remainingStart,
      remainingEnd,
    ];
  }
}

function _maxBigInt(a, b) { return a > b ? a : b; }
function _minBigInt(a, b) { return a < b ? a : b; }
function _saturatingSub(a, b) { return a > b ? a - b : 0n; }

// ---------------------------------------------------------------------------
// Byte/string/case-insensitive helpers.
// ---------------------------------------------------------------------------

function _toBytes(value) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof Uint8Array) return Buffer.from(value.buffer, value.byteOffset, value.byteLength);
  if (typeof value === 'string') return Buffer.from(value, 'utf8');
  throw new TypeError(`expected bytes/str, got ${typeof value}`);
}

function _splitPayload(payload) {
  if (payload instanceof Uint8Array && !Buffer.isBuffer(payload)) {
    payload = Buffer.from(payload.buffer, payload.byteOffset, payload.byteLength);
  }
  const eq = payload.indexOf(0x3d);
  if (eq < 0) return null;
  return [Buffer.from(payload.subarray(0, eq)), Buffer.from(payload.subarray(eq + 1))];
}

function _parseSourceRealtime(value) {
  const buf = _toBytes(value);
  if (buf.length === 0) return null;
  const text = buf.toString('latin1');
  if (!/^[0-9]+$/.test(text)) return null;
  try {
    return BigInt(text);
  } catch {
    return null;
  }
}

function _findAsciiCaseInsensitive(haystack, needle, startAt = 0) {
  if (needle.length === 0) return startAt;
  if (haystack.length - startAt < needle.length) return -1;
  const last = haystack.length - needle.length;
  for (let i = startAt; i <= last; i++) {
    if (_asciiEqualFold(haystack, i, needle, 0, needle.length)) return i;
  }
  return -1;
}

function _containsAsciiCaseInsensitive(haystack, needle) {
  return _findAsciiCaseInsensitive(haystack, needle, 0) !== -1;
}

function _asciiEqualFold(a, aOff, b, bOff, len) {
  for (let i = 0; i < len; i++) {
    if (!_asciiEq(a[aOff + i], b[bOff + i])) return false;
  }
  return true;
}

function _asciiEq(x, y) {
  if (x >= 0x41 && x <= 0x5A) x += 0x20;
  if (y >= 0x41 && y <= 0x5A) y += 0x20;
  return x === y;
}

// ---------------------------------------------------------------------------
// Validation helpers (mirrors Rust validate_query / validate_indexed_query).
// ---------------------------------------------------------------------------

function _validateQuery(query) {
  if (
    query.afterRealtimeUsec !== null
    && query.beforeRealtimeUsec !== null
    && query.afterRealtimeUsec > query.beforeRealtimeUsec
  ) {
    throw new ExplorerError('after_realtime_usec must be <= before_realtime_usec');
  }
  for (const f of query.filters) {
    if (f.field.length === 0 || f.field.includes(0x3d)) {
      throw new ExplorerError("filter field must be non-empty and must not contain '='");
    }
  }
  for (const f of query.facets) {
    if (f.length === 0 || f.includes(0x3d)) {
      throw new ExplorerError("facet and histogram fields must be non-empty and must not contain '='");
    }
  }
  if (query.histogram !== null) {
    if (query.histogram.length === 0 || query.histogram.includes(0x3d)) {
      throw new ExplorerError("facet and histogram fields must be non-empty and must not contain '='");
    }
  }
  const seen = new Set();
  for (const f of query.facets) {
    const key = f.toString('hex');
    if (seen.has(key)) {
      throw new ExplorerError('facet fields must not be duplicated');
    }
    seen.add(key);
  }
}

function _validateNoDebugColumnCollection(query) {
  if (query.debugCollectColumnFieldsByRowTraversal) {
    throw new ExplorerUnsupported(
      'debug_collect_column_fields_by_row_traversal is a debug-only discrepancy tool; '
      + 'production explorer queries must use FIELD-index column catalogs instead',
    );
  }
}

function _validateIndexedQuery(query) {
  if (query.fieldMode !== ExplorerFieldMode.AllValues) {
    throw new ExplorerUnsupported('indexed explorer strategy requires ExplorerFieldMode.AllValues');
  }
  if (_queryHasFts(query)) {
    throw new ExplorerUnsupported('indexed explorer strategy does not support FTS');
  }
  if (
    query.useSourceRealtime
    && (query.afterRealtimeUsec !== null
      || query.beforeRealtimeUsec !== null
      || query.histogram !== null)
  ) {
    throw new ExplorerUnsupported(
      'indexed explorer strategy requires commit realtime for time-bounded facets and histograms',
    );
  }
}

function _queryHasFts(query) {
  return query.ftsTerms.length > 0 || query.ftsPatterns.length > 0 || query.ftsNegativePatterns.length > 0;
}

function _queryHasPositiveFts(query) {
  if (query.ftsTerms.length > 0) return query.ftsTerms.some((t) => !t.negative);
  return query.ftsPatterns.length > 0;
}

function _rowRejectedByFts(query, ftsMatches, ftsNegative) {
  if (!_queryHasFts(query)) return false;
  if (ftsNegative) return true;
  if (_queryHasPositiveFts(query) && !ftsMatches) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Histogram bucketing (mirrors Rust new_histogram / histogram_* helpers).
// ---------------------------------------------------------------------------

function _histogramBounds(query) {
  const start = query.histogramAfterRealtimeUsec !== null
    ? query.histogramAfterRealtimeUsec
    : (query.afterRealtimeUsec !== null ? query.afterRealtimeUsec : 0n);
  let end;
  if (query.histogramBeforeRealtimeUsec !== null) end = query.histogramBeforeRealtimeUsec;
  else if (query.beforeRealtimeUsec !== null) end = query.beforeRealtimeUsec;
  else end = start + EXPLORER_HISTOGRAM_DEFAULT_WINDOW_USEC;
  if (end <= start) return [start, start + 1n];
  return [start, end];
}

function _histogramBarWidthUsec(after, before, targetBuckets) {
  const usecPerSec = 1_000_000n;
  const duration = before - after;
  for (let i = _VALID_HISTOGRAM_BAR_SECONDS.length - 1; i >= 0; i--) {
    const width = BigInt(_VALID_HISTOGRAM_BAR_SECONDS[i]) * usecPerSec;
    if (width !== 0n && duration / width >= BigInt(targetBuckets)) return width;
  }
  return usecPerSec;
}

function _histogramSlotBaselineUsec(value, width) {
  let w = width;
  if (w <= 0n) w = 1n;
  return value - (value % w);
}

export function _newHistogram(field, query) {
  const [startRaw, endRaw] = _histogramBounds(query);
  const target = Math.max(1, Math.trunc(query.histogramTargetBuckets));
  let width = _histogramBarWidthUsec(startRaw, endRaw, target);
  let start = _histogramSlotBaselineUsec(startRaw, width);
  let end = _histogramSlotBaselineUsec(endRaw, width) + width;
  let bucketCount = Number((end - start) / width) + 1;
  if (bucketCount > EXPLORER_HISTOGRAM_MAX_BUCKETS) {
    bucketCount = EXPLORER_HISTOGRAM_MAX_BUCKETS;
    width = (end - start) / 1000n;
    if (width < 1n) width = 1n;
    end = start + width * 1000n;
  }
  const buckets = [];
  for (let i = 0; i < bucketCount; i++) {
    const bucketStart = start + width * BigInt(i);
    const bucketEnd = (i + 1 === bucketCount) ? end + 1n : bucketStart + width;
    buckets.push(new ExplorerHistogramBucket(bucketStart, bucketEnd));
  }
  return new ExplorerHistogram(field, buckets);
}

function _histogramBucketIndexFromBounds(realtimeUsec, start, width, count) {
  if (count === 0) return null;
  let w = width;
  if (w <= 0n) w = 1n;
  if (realtimeUsec < start) return 0;
  const idx = Number((realtimeUsec - start) / w);
  if (idx >= count) return count - 1;
  return idx;
}

function _histogramBucketIndex(histogram, realtimeUsec) {
  if (histogram.buckets.length === 0) return null;
  const first = histogram.buckets[0];
  let width = first.endRealtimeUsec - first.startRealtimeUsec;
  if (width <= 0n) width = 1n;
  return _histogramBucketIndexFromBounds(realtimeUsec, first.startRealtimeUsec, width, histogram.buckets.length);
}

// ---------------------------------------------------------------------------
// Time-window helpers (mirrors Rust timestamp_in_range / stop_by_commit_time
// / skip_by_commit_time / row_within_anchor / should_stop_when_rows_full).
// ---------------------------------------------------------------------------

function _timestampInRange(query, ts) {
  if (query.afterRealtimeUsec !== null && ts < query.afterRealtimeUsec) return false;
  if (query.beforeRealtimeUsec !== null && ts > query.beforeRealtimeUsec) return false;
  return true;
}

function _stopByCommitTime(query, commitRealtime) {
  if (query.direction === Direction.Forward) {
    if (query.beforeRealtimeUsec === null) return false;
    return commitRealtime > query.beforeRealtimeUsec + query.realtimeSlackUsec;
  }
  if (query.afterRealtimeUsec === null) return false;
  return commitRealtime < query.afterRealtimeUsec;
}

function _skipByCommitTime(query, commitRealtime) {
  if (query.direction === Direction.Forward) {
    if (query.afterRealtimeUsec === null) return false;
    return commitRealtime < query.afterRealtimeUsec;
  }
  if (query.beforeRealtimeUsec === null) return false;
  return commitRealtime > query.beforeRealtimeUsec + query.realtimeSlackUsec;
}

function _rowWithinAnchor(query, realtimeUsec) {
  if (query.anchor.kind !== ExplorerAnchorKind.Realtime) return true;
  if (query.direction === Direction.Forward) return realtimeUsec > query.anchor.realtimeUsec;
  return realtimeUsec <= query.anchor.realtimeUsec;
}

function _rowCandidateToKeep(query, rows, realtimeUsec) {
  if (query.limit === 0) return false;
  if (!_rowWithinAnchor(query, realtimeUsec)) return false;
  if (rows.length < query.limit) return true;
  if (query.direction === Direction.Backward) {
    let oldest = null;
    for (const r of rows) { if (oldest === null || r.realtimeUsec < oldest) oldest = r.realtimeUsec; }
    return realtimeUsec >= oldest;
  }
  let newest = null;
  for (const r of rows) { if (newest === null || r.realtimeUsec > newest) newest = r.realtimeUsec; }
  return realtimeUsec <= newest;
}

function _shouldStopWhenRowsFull(query, rows, effectiveRealtime, rowsMatched) {
  if (!query.stopWhenRowsFull || query.limit === 0 || rows.length < query.limit) return false;
  const every = Math.max(1, Math.trunc(query.stopWhenRowsFullCheckEvery));
  if (rowsMatched === 0n || rowsMatched % BigInt(every) !== 0n) return false;
  if (query.direction === Direction.Backward) {
    let oldest = null;
    for (const r of rows) { if (oldest === null || r.realtimeUsec < oldest) oldest = r.realtimeUsec; }
    if (oldest === null) return false;
    return effectiveRealtime < oldest - query.realtimeSlackUsec;
  }
  let newest = null;
  for (const r of rows) { if (newest === null || r.realtimeUsec > newest) newest = r.realtimeUsec; }
  if (newest === null) return false;
  return effectiveRealtime > newest + query.realtimeSlackUsec;
}

function _effectiveRealtimeFromScan(sourceRealtime, commitRealtime) {
  if (sourceRealtime !== null && sourceRealtime !== 0n && sourceRealtime < commitRealtime) {
    return sourceRealtime;
  }
  return commitRealtime;
}

function _recordLastRealtime(stats, commitRealtime) {
  if (commitRealtime > stats.lastRealtimeUsec) stats.lastRealtimeUsec = commitRealtime;
}

function _recordSourceRealtimeDelta(stats, sourceRealtime, commitRealtime) {
  if (sourceRealtime === null || sourceRealtime === 0n || sourceRealtime >= commitRealtime) return;
  const delta = commitRealtime - sourceRealtime;
  if (delta > stats.maxSourceRealtimeDeltaUsec) stats.maxSourceRealtimeDeltaUsec = delta;
}

function _queryNeedsMainPass(query) {
  return query.limit > 0 || query.histogram !== null;
}

function _queryNeedsSourceRealtimeMain(query) {
  return query.useSourceRealtime && (
    query.afterRealtimeUsec !== null
    || query.beforeRealtimeUsec !== null
    || query.histogram !== null
    || query.limit > 0
  );
}

function _facetPassNeedsSourceRealtime(query) {
  return query.useSourceRealtime && (query.afterRealtimeUsec !== null || query.beforeRealtimeUsec !== null);
}

// ---------------------------------------------------------------------------
// FTS matching for a single value (mirrors Rust match_fts_query).
// ---------------------------------------------------------------------------

function _matchFtsQuery(value, query) {
  if (query.ftsTerms.length > 0) {
    for (const term of query.ftsTerms) {
      if (term.matches(value)) {
        return term.negative ? { positive: false, negative: true } : { positive: true, negative: false };
      }
    }
    return { positive: false, negative: false };
  }
  for (const pat of query.ftsNegativePatterns) {
    if (pat.length > 0 && _containsAsciiCaseInsensitive(value, pat)) {
      return { positive: false, negative: true };
    }
  }
  for (const pat of query.ftsPatterns) {
    if (pat.length > 0 && _containsAsciiCaseInsensitive(value, pat)) {
      return { positive: true, negative: false };
    }
  }
  return { positive: false, negative: false };
}

// ---------------------------------------------------------------------------
// Offset-class classification cache (mirrors Rust OffsetClassCache).
// ---------------------------------------------------------------------------

class _OffsetClassCache {
  constructor() {
    this._slots = new Map();
  }
  lookup(offset) {
    if (offset === 0n) return undefined;
    return this._slots.get(Number(offset));
  }
  insert(offset, classValue) {
    if (offset === 0n) return;
    this._slots.set(Number(offset), classValue);
  }
}

// ---------------------------------------------------------------------------
// Facet pass group (mirrors Rust FacetPassGroup / facet_pass_groups).
// ---------------------------------------------------------------------------

class _FacetPassGroup {
  constructor(excludedField, facetIndices) {
    this.excludedField = excludedField;
    this.facetIndices = facetIndices;
  }
}

function _facetPassGroups(query) {
  const filterFieldKeys = new Set();
  for (const f of query.filters) filterFieldKeys.add(f.field.toString('hex'));
  const groups = [];
  for (let idx = 0; idx < query.facets.length; idx++) {
    const facet = query.facets[idx];
    let excluded = null;
    if (query.excludeFacetFieldFilters && filterFieldKeys.has(facet.toString('hex'))) {
      excluded = facet;
    }
    let merged = false;
    const excludedHex = excluded === null ? null : excluded.toString('hex');
    for (const g of groups) {
      const gHex = g.excludedField === null ? null : g.excludedField.toString('hex');
      if (gHex === excludedHex) {
        g.facetIndices.push(idx);
        merged = true;
        break;
      }
    }
    if (!merged) {
      groups.push(new _FacetPassGroup(excluded, [idx]));
    }
  }
  return groups;
}

function _canRunCombinedPass(groups) {
  for (const g of groups) if (g.excludedField !== null) return false;
  return true;
}

function _combinedFacetIndices(groups) {
  const out = [];
  for (const g of groups) out.push(...g.facetIndices);
  return out;
}

// ---------------------------------------------------------------------------
// Accumulator (mirrors Rust ExplorerAccumulator).
// ---------------------------------------------------------------------------

class _ExplorerAccumulator {
  constructor(histogram) {
    this.fieldLookup = new Map();
    this.fields = [];
    this.flags = [];
    this.lastSeenRowIds = [];
    this.unsetCounts = [];
    this.valuesByField = [];
    this.valueCounts = [];
    this.valueFieldIndices = [];
    this.valueLabels = [];
    this.valueFtsMatches = [];
    this.valueSourceRealtime = [];
    this.valueHistogramBuckets = [];
    this.fieldHistogramUnsetBuckets = [];
    this.offsetCache = new _OffsetClassCache();
    if (histogram !== null && histogram.buckets.length > 0) {
      const first = histogram.buckets[0];
      this.histogramStartRealtimeUsec = first.startRealtimeUsec;
      let width = first.endRealtimeUsec - first.startRealtimeUsec;
      if (width <= 0n) width = 1n;
      this.histogramBucketWidthUsec = width;
      this.histogramBucketCount = histogram.buckets.length;
    } else {
      this.histogramStartRealtimeUsec = 0n;
      this.histogramBucketWidthUsec = 1n;
      this.histogramBucketCount = 0;
    }
    this.requiredIdentityCount = 0;
  }

  addField(field, flags) {
    const key = field.toString('hex');
    const existing = this.fieldLookup.get(key);
    if (existing !== undefined) {
      const hadRequired = this.flags[existing] !== 0;
      this.flags[existing] |= flags;
      if ((flags & FACET_HISTOGRAM) !== 0 && this.fieldHistogramUnsetBuckets[existing] === undefined) {
        this.fieldHistogramUnsetBuckets[existing] = new Array(this.histogramBucketCount).fill(0n);
      }
      if (!hadRequired && this.flags[existing] !== 0) this.requiredIdentityCount += 1;
      return existing;
    }
    const idx = this.fields.length;
    this.fieldLookup.set(key, idx);
    this.fields.push(field);
    this.flags.push(flags);
    this.lastSeenRowIds.push(0n);
    this.unsetCounts.push(0n);
    this.valuesByField.push([]);
    if ((flags & FACET_HISTOGRAM) !== 0) {
      this.fieldHistogramUnsetBuckets.push(new Array(this.histogramBucketCount).fill(0n));
    } else {
      this.fieldHistogramUnsetBuckets.push(undefined);
    }
    if (flags !== 0) this.requiredIdentityCount += 1;
    return idx;
  }

  addValue(fieldIdx, value, ftsMatches) {
    const valueIndex = this.valueCounts.length;
    const flags = this.flags[fieldIdx];
    this.valueCounts.push(0n);
    this.valueFieldIndices.push(fieldIdx);
    this.valueLabels.push(value);
    this.valueFtsMatches.push(ftsMatches);
    if ((flags & FACET_SOURCE_REALTIME) !== 0) {
      this.valueSourceRealtime.push(_parseSourceRealtime(value));
    } else {
      this.valueSourceRealtime.push(null);
    }
    if ((flags & FACET_HISTOGRAM) !== 0) {
      this.valueHistogramBuckets.push(new Array(this.histogramBucketCount).fill(0n));
    } else {
      this.valueHistogramBuckets.push(undefined);
    }
    this.valuesByField[fieldIdx].push(valueIndex);
    return valueIndex;
  }

  markFieldSeen(fieldIdx, rowId) {
    if (this.lastSeenRowIds[fieldIdx] === rowId) return false;
    this.lastSeenRowIds[fieldIdx] = rowId;
    return true;
  }

  applyValue(valueIndex, realtimeUsec, stats) {
    const fieldIdx = this.valueFieldIndices[valueIndex];
    const flags = this.flags[fieldIdx];
    if ((flags & FACET_PUBLIC) !== 0) {
      this.valueCounts[valueIndex] += 1n;
      stats.facetUpdates += 1n;
    }
    if ((flags & FACET_HISTOGRAM) !== 0 && realtimeUsec !== null) {
      const buckets = this.valueHistogramBuckets[valueIndex];
      if (buckets !== undefined) {
        const idx = _histogramBucketIndexFromBounds(
          realtimeUsec, this.histogramStartRealtimeUsec, this.histogramBucketWidthUsec, buckets.length,
        );
        if (idx !== null) {
          buckets[idx] += 1n;
          stats.histogramUpdates += 1n;
        }
      }
    }
  }

  finishFacetRow(rowId, stats) {
    for (let i = 0; i < this.fields.length; i++) {
      if ((this.flags[i] & FACET_PUBLIC) === 0) continue;
      if (this.lastSeenRowIds[i] !== rowId) {
        this.unsetCounts[i] += 1n;
        stats.facetUpdates += 1n;
      }
    }
  }

  finishHistogramRow(rowId, realtimeUsec, stats) {
    for (let i = 0; i < this.fields.length; i++) {
      if ((this.flags[i] & FACET_HISTOGRAM) === 0) continue;
      if (this.lastSeenRowIds[i] === rowId) continue;
      const buckets = this.fieldHistogramUnsetBuckets[i];
      if (buckets === undefined) continue;
      const idx = _histogramBucketIndexFromBounds(
        realtimeUsec, this.histogramStartRealtimeUsec, this.histogramBucketWidthUsec, buckets.length,
      );
      if (idx !== null) {
        buckets[idx] += 1n;
        stats.histogramUpdates += 1n;
      }
    }
  }

  finishFacets(result) {
    for (let i = 0; i < this.fields.length; i++) {
      if ((this.flags[i] & FACET_PUBLIC) === 0) continue;
      const values = new Map();
      for (const valueIndex of this.valuesByField[i]) {
        const count = this.valueCounts[valueIndex];
        if (count !== 0n) {
          _incrementCounter(values, this.valueLabels[valueIndex], count);
        }
      }
      if (this.unsetCounts[i] !== 0n) {
        _incrementCounter(values, UNSET_VALUE, this.unsetCounts[i]);
      }
      result.facets.set(this.fields[i].toString('hex'), values);
    }
  }

  finishHistogram(histogram) {
    if (histogram === null) return;
    for (const buckets of this.fieldHistogramUnsetBuckets) {
      if (buckets === undefined) continue;
      for (let idx = 0; idx < buckets.length; idx++) {
        const count = buckets[idx];
        if (count === 0n) continue;
        const bucket = histogram.buckets[idx];
        _incrementCounter(bucket.values, UNSET_VALUE, count);
      }
    }
    for (let valueIndex = 0; valueIndex < this.valueHistogramBuckets.length; valueIndex++) {
      const buckets = this.valueHistogramBuckets[valueIndex];
      if (buckets === undefined) continue;
      for (let idx = 0; idx < buckets.length; idx++) {
        const count = buckets[idx];
        if (count === 0n) continue;
        const bucket = histogram.buckets[idx];
        _incrementCounter(bucket.values, this.valueLabels[valueIndex], count);
      }
    }
  }
}

function _incrementCounter(counter, key, delta) {
  // Map keys are hex strings (the value bytes' hex representation).
  // This mirrors Python's bytes-as-dict-key behavior and Rust's Vec<u8>
  // as HashMap key (both use value equality), so cross-language parity
  // is preserved. The hex form is the only way to get value-based
  // equality from native JavaScript Map, which uses reference equality
  // for object keys.
  const k = Buffer.isBuffer(key) ? key.toString('hex') : String(key);
  const existing = counter.get(k);
  if (existing === undefined) {
    counter.set(k, delta);
  } else {
    counter.set(k, existing + delta);
  }
}

function _addSpecialHistogramValue(histogram, realtimeUsec, value, count, stats) {
  if (histogram === null) return;
  const idx = _histogramBucketIndex(histogram, realtimeUsec);
  if (idx === null) return;
  _incrementCounter(histogram.buckets[idx].values, value, BigInt(count));
  stats.histogramUpdates += 1n;
}

function _addEstimatedHistogramRange(histogram, fromRealtimeUsec, toRealtimeUsec, entries, stats) {
  if (histogram === null) return;
  entries = BigInt(entries);
  if (entries === 0n || fromRealtimeUsec >= toRealtimeUsec) return;
  if (histogram.buckets.length === 0) return;
  const first = histogram.buckets[0];
  const last = histogram.buckets[histogram.buckets.length - 1];
  const start = _maxBigInt(fromRealtimeUsec, first.startRealtimeUsec);
  const end = _minBigInt(toRealtimeUsec, last.endRealtimeUsec);
  if (start >= end) return;
  const total = _maxBigInt(1n, end - start);
  let touched = 0n;
  for (const bucket of histogram.buckets) {
    if (bucket.startRealtimeUsec > end) break;
    const overlapStart = _maxBigInt(bucket.startRealtimeUsec, start);
    const overlapEnd = _minBigInt(bucket.endRealtimeUsec, end);
    if (overlapStart >= overlapEnd) continue;
    const bucketEntries = ((overlapEnd - overlapStart) * entries) / total;
    if (bucketEntries !== 0n) {
      _incrementCounter(bucket.values, EXPLORER_ESTIMATED_VALUE, bucketEntries);
    }
    touched += 1n;
  }
  stats.histogramUpdates += touched;
}

// ---------------------------------------------------------------------------
// Reader shim helpers (mirrors Python's _configure_filters and similar).
// ---------------------------------------------------------------------------

function _flushReaderFilters(reader) {
  reader.flushMatches();
}

function _readerFilterMatches(reader) {
  if (reader.filter === null) return true;
  try {
    const entry = reader.getEntry();
    if (!entry) return true;
    return reader.filter.matches(entry);
  } catch {
    return true;
  }
}

function _configureFilters(reader, query, excludedField) {
  _flushReaderFilters(reader);
  for (const f of query.filters) {
    if (excludedField !== null && f.field.equals(excludedField)) continue;
    if (f.values.length === 0) continue;
    for (const v of f.values) {
      const payload = Buffer.alloc(f.field.length + 1 + v.length);
      f.field.copy(payload, 0);
      payload[f.field.length] = 0x3d;
      v.copy(payload, f.field.length + 1);
      reader.addMatch(payload);
    }
  }
}

function _seekForExplorer(reader, query) {
  // Rust ignores the anchor when stop_when_rows_full is false (always Auto).
  // Mirrors explorer.rs:2096-2098.
  const useAnchor = query.stopWhenRowsFull ? query.anchor : ExplorerAnchor.auto();
  if (query.direction === Direction.Forward) {
    if (useAnchor.kind === ExplorerAnchorKind.Realtime) {
      reader.seekRealtimeUsec(useAnchor.realtimeUsec);
    } else if (useAnchor.kind === ExplorerAnchorKind.Tail) {
      reader.seekTail();
    } else {
      if (query.afterRealtimeUsec !== null) {
        const slack = query.realtimeSlackUsec;
        const after = query.afterRealtimeUsec;
        const target = after > slack ? after - slack : 0n;
        reader.seekRealtimeUsec(target);
      } else {
        reader.seekHead();
      }
    }
  } else {
    if (useAnchor.kind === ExplorerAnchorKind.Realtime) {
      reader.seekRealtimeUsec(useAnchor.realtimeUsec);
    } else if (useAnchor.kind === ExplorerAnchorKind.Head) {
      reader.seekHead();
    } else {
      if (query.beforeRealtimeUsec !== null) {
        reader.seekRealtimeUsec(query.beforeRealtimeUsec + query.realtimeSlackUsec);
      } else {
        reader.seekTail();
      }
    }
  }
}

function _stepExplorer(reader, direction) {
  return direction === Direction.Forward ? reader.next() : reader.previous();
}

function _currentExplorerRow(reader, realtimeUsec, stats, expand = true) {
  const cursor = reader.getCursor();
  if (!expand) {
    return new ExplorerRow(realtimeUsec, cursor || '', []);
  }
  const payloads = reader.collectEntryPayloads();
  stats.returnedRowExpansions += 1n;
  return new ExplorerRow(realtimeUsec, cursor || '', payloads);
}

// ---------------------------------------------------------------------------
// Data-was-compressed probe. The Node reader's parseDataPayload always
// decompresses; we read the object header's flag byte directly to know
// whether this DATA was compressed (and to count payloads_decompressed
// in stats). We only call this when the caller has a reason to inspect
// compression state (e.g. they are about to decompress for filtering).
// ---------------------------------------------------------------------------

function _dataObjectWasCompressed(reader, dataOffset) {
  if (typeof reader._dataObjectWasCompressedAt === 'function') {
    return reader._dataObjectWasCompressedAt(dataOffset);
  }
  return false;
}

// ---------------------------------------------------------------------------
// Row scan: walk DATA objects, classify each, and apply matching values.
// Compressed DATA stays compressed unless the value is needed for
// filtering/faceting/FTS/display (AGENTS.md perf contract).
// ---------------------------------------------------------------------------

function _scanRowData(reader, query, accumulator, rowId, apply, stats, needsFts) {
  // Mirror Rust scan_current_row L2046: every visited row counts.
  stats.rowsExamined += 1n;
  if (accumulator.requiredIdentityCount === 0 && !needsFts) {
    return { ftsMatches: false, ftsNegative: false };
  }
  let ftsMatches = false;
  let ftsNegative = false;
  let dataOffsets;
  try {
    dataOffsets = reader._currentEntryDataOffsets();
  } catch {
    return { ftsMatches, ftsNegative };
  }
  for (const dataOffset of dataOffsets) {
    stats.dataRefsSeen += 1n;
    let classValue = accumulator.offsetCache.lookup(dataOffset);
    if (classValue === undefined) {
      stats.dataCacheMisses += 1n;
      let payload;
      try {
        payload = reader._readDataPayloadAt(dataOffset);
      } catch {
        continue;
      }
      stats.dataPayloadsLoaded += 1n;
      if (_dataObjectWasCompressed(reader, dataOffset)) {
        stats.payloadsDecompressed += 1n;
      }
      const split = _splitPayload(payload);
      if (split === null) {
        if (needsFts) {
          stats.ftsScans += 1n;
          const { positive, negative } = _matchFtsQuery(payload, query);
          if (negative) {
            classValue = _OFFSET_CLASS_FTS_NEGATIVE;
            ftsNegative = true;
          } else if (positive) {
            classValue = _OFFSET_CLASS_FTS_MATCH;
            ftsMatches = true;
          } else {
            classValue = _OFFSET_CLASS_IRRELEVANT;
          }
        } else {
          classValue = _OFFSET_CLASS_IRRELEVANT;
        }
        accumulator.offsetCache.insert(dataOffset, classValue);
        stats.dataObjectsClassified += 1n;
        continue;
      }
      const [field, value] = split;
      let positive = false;
      let negative = false;
      if (needsFts) {
        stats.ftsScans += 1n;
        const m = _matchFtsQuery(value, query);
        positive = m.positive;
        negative = m.negative;
      }
      if (negative) {
        classValue = _OFFSET_CLASS_FTS_NEGATIVE;
        ftsNegative = true;
      } else {
        const fieldIdx = accumulator.fieldLookup.get(field.toString('hex'));
        if (fieldIdx !== undefined) {
          const valueIndex = accumulator.addValue(fieldIdx, value, positive);
          classValue = _OFFSET_CLASS_VALUE_BASE + valueIndex;
        } else if (positive) {
          classValue = _OFFSET_CLASS_FTS_MATCH;
          ftsMatches = true;
        } else {
          classValue = _OFFSET_CLASS_IRRELEVANT;
        }
      }
      accumulator.offsetCache.insert(dataOffset, classValue);
      stats.dataObjectsClassified += 1n;
      if (positive) ftsMatches = true;
      if (negative) ftsNegative = true;
      if (classValue >= _OFFSET_CLASS_VALUE_BASE) {
        _handleValueClass(
          accumulator, classValue - _OFFSET_CLASS_VALUE_BASE, rowId, query, apply, stats,
        );
      }
      continue;
    }
    stats.dataCacheHits += 1n;
    if (classValue === _OFFSET_CLASS_IRRELEVANT) {
      stats.dataRefsSkipped += 1n;
    } else if (classValue === _OFFSET_CLASS_FTS_NEGATIVE) {
      ftsNegative = true;
    } else if (classValue === _OFFSET_CLASS_FTS_MATCH) {
      ftsMatches = true;
    } else if (classValue >= _OFFSET_CLASS_VALUE_BASE) {
      _handleValueClass(
        accumulator, classValue - _OFFSET_CLASS_VALUE_BASE, rowId, query, apply, stats,
      );
    }
  }
  return { ftsMatches, ftsNegative };
}

function _handleValueClass(accumulator, valueIndex, rowId, query, apply, stats) {
  // Mirror Rust handle_row_value_class (L2611-2643).
  const fieldIndex = accumulator.valueFieldIndices[valueIndex];
  const useFirstValue = query.fieldMode === ExplorerFieldMode.FirstValue;
  const flags = accumulator.flags[fieldIndex];
  const isRequiredRole = (flags & (FACET_PUBLIC | FACET_HISTOGRAM)) !== 0;
  let firstForField = true;
  if (useFirstValue || isRequiredRole) {
    firstForField = accumulator.markFieldSeen(fieldIndex, rowId);
  }
  if (useFirstValue && !firstForField) return;
  if (apply === 'immediate') {
    accumulator.applyValue(valueIndex, null, stats);
    return;
  }
  apply.deferred.push([valueIndex, accumulator.valueSourceRealtime[valueIndex]]);
}

function _pickSourceRealtime(deferred) {
  for (const [, sourceRealtime] of deferred) {
    if (sourceRealtime !== null && sourceRealtime !== 0n) return sourceRealtime;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Traversal strategy entry point. Splits into combined or split passes.
// ---------------------------------------------------------------------------

function _exploreFileReader(reader, query, strategy, control) {
  _validateNoDebugColumnCollection(query);
  if (strategy === ExplorerStrategy.Traversal) {
    return _exploreTraversal(reader, query, control);
  }
  if (strategy === ExplorerStrategy.Index) {
    return _exploreIndexed(reader, query, control);
  }
  if (strategy === ExplorerStrategy.Compare) {
    return _exploreCompare(reader, query);
  }
  throw new ExplorerError(`unsupported explorer strategy ${strategy}`);
}

function _exploreTraversal(reader, query, control) {
  _validateQuery(query);
  const result = _explorerResultForQuery(reader, query);
  const groups = _facetPassGroups(query);
  if (_canRunCombinedPass(groups)) {
    _exploreTraversalCombined(reader, query, groups, result, control);
  } else {
    _exploreTraversalSplit(reader, query, groups, result, control);
  }
  _flushReaderFilters(reader);
  return result;
}

function _explorerResultForQuery(reader, query) {
  const result = new ExplorerResult();
  // Column catalog from the FIELD hash-table index, not from row traversal.
  // Mirrors the FIELD-index path documented in api-diff-inventory.md
  // (already present in the reader, item refuted from "non-gap" list).
  if (typeof reader._enumerateFieldsIndexed === 'function') {
    const fields = reader._enumerateFieldsIndexed();
    for (const f of fields) result.columnFields.add(f);
  } else if (typeof reader.enumerateFields === 'function') {
    const fields = reader.enumerateFields();
    for (const f of fields) result.columnFields.add(f);
  }
  if (query.histogram !== null) {
    result.histogram = _newHistogram(query.histogram, query);
  }
  return result;
}

function _exploreTraversalCombined(reader, query, groups, result, control) {
  const facetIndices = _combinedFacetIndices(groups);
  if (!_queryNeedsMainPass(query) && facetIndices.length === 0) return;
  _configureFilters(reader, query, null);
  const accumulator = _buildCombinedAccumulator(query, facetIndices, result.histogram);
  _scanExplorerCombined(reader, query, accumulator, result, facetIndices.length > 0, control);
  accumulator.finishFacets(result);
  accumulator.finishHistogram(result.histogram);
}

function _exploreTraversalSplit(reader, query, groups, result, control) {
  if (_queryNeedsMainPass(query)) {
    _configureFilters(reader, query, null);
    const accumulator = _buildMainAccumulator(query, result.histogram);
    _scanExplorerMain(reader, query, accumulator, result, control);
    accumulator.finishHistogram(result.histogram);
  }
  for (const group of groups) {
    if (control !== null && control._stopped) break;
    _configureFilters(reader, query, group.excludedField);
    const accumulator = _buildFacetAccumulator(
      query, group.facetIndices, _facetPassNeedsSourceRealtime(query),
    );
    _scanExplorerFacet(reader, query, accumulator, result.stats, control);
    accumulator.finishFacets(result);
  }
}

function _buildMainAccumulator(query, histogram) {
  const acc = new _ExplorerAccumulator(histogram);
  if (query.histogram !== null) acc.addField(query.histogram, FACET_HISTOGRAM);
  if (_queryNeedsSourceRealtimeMain(query)) acc.addField(SOURCE_REALTIME_FIELD, FACET_SOURCE_REALTIME);
  return acc;
}

function _buildFacetAccumulator(query, facetIndices, includeSourceRealtime) {
  const acc = new _ExplorerAccumulator(null);
  for (const idx of facetIndices) {
    if (idx >= 0 && idx < query.facets.length) acc.addField(query.facets[idx], FACET_PUBLIC);
  }
  if (includeSourceRealtime) acc.addField(SOURCE_REALTIME_FIELD, FACET_SOURCE_REALTIME);
  return acc;
}

function _buildCombinedAccumulator(query, facetIndices, histogram) {
  const acc = new _ExplorerAccumulator(histogram);
  if (query.histogram !== null) acc.addField(query.histogram, FACET_HISTOGRAM);
  for (const idx of facetIndices) {
    if (idx >= 0 && idx < query.facets.length) acc.addField(query.facets[idx], FACET_PUBLIC);
  }
  if (_queryNeedsSourceRealtimeMain(query) || _facetPassNeedsSourceRealtime(query)) {
    acc.addField(SOURCE_REALTIME_FIELD, FACET_SOURCE_REALTIME);
  }
  return acc;
}

function _scanExplorerMain(reader, query, accumulator, result, control) {
  _seekForExplorer(reader, query);
  const useFirstValue = query.fieldMode === ExplorerFieldMode.FirstValue;
  const needsFts = _queryHasFts(query);
  let rowId = 0n;
  let rowsSeen = 0n;
  const apply = { deferred: [] };
  while (true) {
    if (!_stepExplorer(reader, query.direction)) break;
    rowsSeen += 1n;
    if (control !== null && control.shouldStopAfterRows(rowsSeen, result.stats)) break;
    const commitRealtime = reader.getRealtimeUsec();
    if (commitRealtime === 0n) continue;
    if (_stopByCommitTime(query, commitRealtime)) break;
    if (_skipByCommitTime(query, commitRealtime)) continue;
    if (!_readerFilterMatches(reader)) continue;
    apply.deferred.length = 0;
    rowId += 1n;
    const { ftsMatches, ftsNegative } = _scanRowData(
      reader, query, accumulator, rowId, apply, result.stats, needsFts,
    );
    const sourceRealtime = _pickSourceRealtime(apply.deferred);
    const effective = _effectiveRealtimeFromScan(sourceRealtime, commitRealtime);
    _recordSourceRealtimeDelta(result.stats, sourceRealtime, commitRealtime);
    if (!_timestampInRange(query, effective)) continue;
    if (_rowRejectedByFts(query, ftsMatches, ftsNegative)) continue;
    _recordLastRealtime(result.stats, commitRealtime);
    result.stats.rowsMatched += 1n;
    let stopAfterMatched = false;
    if (control !== null) {
      stopAfterMatched = control.emitMatchedRow(effective, result.stats.rowsMatched);
    }
    const valueRealtime = query.histogram !== null ? effective : null;
    for (const [valueIndex] of apply.deferred) {
      accumulator.applyValue(valueIndex, valueRealtime, result.stats);
    }
    accumulator.finishHistogramRow(rowId, effective, result.stats);
    if (_rowWithinAnchor(query, effective) && result.rows.length < query.limit) {
      result.rows.push(_currentExplorerRow(reader, effective, result.stats, true));
    }
    if (
      stopAfterMatched
      || _shouldStopWhenRowsFull(query, result.rows, effective, result.stats.rowsMatched)
    ) break;
    if (
      useFirstValue
      && !needsFts
      && accumulator.requiredIdentityCount === 0
      && apply.deferred.length === 0
    ) {
      // Fast-path: required identity count already reached and no FTS;
      // mirror Rust's should_stop_row_scan early-stop bookkeeping.
      // (Not fatal to skip — but we record the early-stop stats so
      // parity with Rust's `early_stop_opportunities` / `early_stops`
      // counters stays close.)
      result.stats.earlyStopOpportunities += 1n;
      result.stats.earlyStops += 1n;
    }
  }
  result.stats.rowsReturned = BigInt(result.rows.length);
}

function _scanExplorerCombined(reader, query, accumulator, result, includeFacets, control) {
  _seekForExplorer(reader, query);
  const useFirstValue = query.fieldMode === ExplorerFieldMode.FirstValue;
  const needsFts = _queryHasFts(query);
  const includeMain = _queryNeedsMainPass(query);
  const sampling = _samplingStateForCombined(query, result, control);
  let rowId = 0n;
  let rowsSeen = 0n;
  const apply = { deferred: [] };
  while (true) {
    if (!_stepExplorer(reader, query.direction)) break;
    rowsSeen += 1n;
    if (control !== null && control.shouldStopAfterRows(rowsSeen, result.stats)) break;
    const commitRealtime = reader.getRealtimeUsec();
    if (commitRealtime === 0n) continue;
    if (_stopByCommitTime(query, commitRealtime)) break;
    if (_skipByCommitTime(query, commitRealtime)) continue;
    if (!_readerFilterMatches(reader)) continue;
    const decision = _combinedSamplingDecision(
      query,
      result.rows,
      commitRealtime,
      _readerCurrentEntrySeqnum(reader),
      sampling,
      control,
    );
    if (decision !== null) {
      const action = _applyCombinedSamplingDecision(
        decision, includeMain, includeFacets, result, commitRealtime,
      );
      if (action === 'skip') continue;
      if (action === 'stop') break;
    }
    apply.deferred.length = 0;
    rowId += 1n;
    const { ftsMatches, ftsNegative } = _scanRowData(
      reader, query, accumulator, rowId, apply, result.stats, needsFts,
    );
    const sourceRealtime = _pickSourceRealtime(apply.deferred);
    const effective = _effectiveRealtimeFromScan(sourceRealtime, commitRealtime);
    _recordSourceRealtimeDelta(result.stats, sourceRealtime, commitRealtime);
    if (!_timestampInRange(query, effective)) continue;
    if (_rowRejectedByFts(query, ftsMatches, ftsNegative)) continue;
    _recordLastRealtime(result.stats, commitRealtime);
    let stopAfterMatched = false;
    if (_queryNeedsMainPass(query)) {
      result.stats.rowsMatched += 1n;
      if (control !== null) {
        stopAfterMatched = control.emitMatchedRow(effective, result.stats.rowsMatched);
      }
    }
    if (includeFacets) result.stats.facetRowsMatched += 1n;
    const valueRealtime = query.histogram !== null ? effective : null;
    for (const [valueIndex] of apply.deferred) {
      accumulator.applyValue(valueIndex, valueRealtime, result.stats);
    }
    if (query.histogram !== null) {
      accumulator.finishHistogramRow(rowId, effective, result.stats);
    }
    if (includeFacets) accumulator.finishFacetRow(rowId, result.stats);
    if (
      _queryNeedsMainPass(query)
      && _rowWithinAnchor(query, effective)
      && result.rows.length < query.limit
    ) {
      result.rows.push(_currentExplorerRow(reader, effective, result.stats, true));
    }
    if (
      stopAfterMatched
      || _shouldStopWhenRowsFull(query, result.rows, effective, result.stats.rowsMatched)
    ) break;
  }
  result.stats.rowsReturned = BigInt(result.rows.length);
}

function _samplingStateForCombined(query, result, control) {
  const bucketCount = result.histogram !== null ? result.histogram.buckets.length : null;
  const sampling = _ExplorerSamplingState.forQuery(query, bucketCount);
  if (control !== null && control.sampling !== null && query.sampling !== null) {
    control.sampling.beginFile(query.sampling);
  }
  return sampling;
}

function _combinedSamplingDecision(query, rows, commitRealtime, seqnum, sampling, control) {
  const candidate = _rowCandidateToKeep(query, rows, commitRealtime);
  if (control !== null && control.sampling !== null) {
    return control.sampling.decide(commitRealtime, seqnum, candidate);
  }
  if (sampling !== null) return sampling.decide(commitRealtime, seqnum, candidate);
  return null;
}

function _readerCurrentEntrySeqnum(reader) {
  if (reader.entryIndex < 0 || reader.entryIndex >= reader.entryOffsets.length) return 0n;
  return reader._u64(Number(reader.entryOffsets[reader.entryIndex]) + OBJECT_HEADER_SIZE);
}

function _applyCombinedSamplingDecision(decision, includeMain, includeFacets, result, commitRealtime) {
  if (decision.action === 'full') {
    if (decision.sampled) result.stats.samplingSampled += 1n;
    return 'scan';
  }
  if (decision.action === 'skip') {
    _recordCombinedUnsampledRow(result.stats, includeMain, includeFacets, commitRealtime, 1n, true);
    _addSpecialHistogramValue(
      result.histogram, commitRealtime, EXPLORER_UNSAMPLED_VALUE, 1n, result.stats,
    );
    return 'skip';
  }
  if (decision.action === 'stop') {
    _recordCombinedUnsampledRow(
      result.stats,
      includeMain,
      includeFacets,
      commitRealtime,
      decision.remainingRows,
      false,
    );
    result.stats.rowsEstimated += decision.remainingRows;
    result.stats.samplingEstimated += decision.remainingRows;
    _addEstimatedHistogramRange(
      result.histogram,
      decision.fromRealtimeUsec,
      decision.toRealtimeUsec,
      decision.remainingRows,
      result.stats,
    );
    return 'stop';
  }
  return 'scan';
}

function _recordCombinedUnsampledRow(
  stats, includeMain, includeFacets, commitRealtime, rowCount, countRowsUnsampled,
) {
  _recordLastRealtime(stats, commitRealtime);
  if (includeMain) stats.rowsMatched += BigInt(rowCount);
  if (includeFacets) stats.facetRowsMatched += BigInt(rowCount);
  if (countRowsUnsampled) stats.rowsUnsampled += BigInt(rowCount);
  stats.samplingUnsampled += 1n;
}

function _scanExplorerFacet(reader, query, accumulator, stats, control) {
  _seekForExplorer(reader, query);
  const needsFts = _queryHasFts(query);
  const deferApply = (
    query.afterRealtimeUsec !== null
    || query.beforeRealtimeUsec !== null
    || needsFts
  );
  let rowId = 0n;
  let rowsSeen = 0n;
  const apply = deferApply ? { deferred: [] } : 'immediate';
  while (true) {
    if (!_stepExplorer(reader, query.direction)) break;
    rowsSeen += 1n;
    if (control !== null && control.shouldStopAfterRows(rowsSeen, stats)) break;
    const commitRealtime = reader.getRealtimeUsec();
    if (commitRealtime === 0n) continue;
    if (_stopByCommitTime(query, commitRealtime)) break;
    if (_skipByCommitTime(query, commitRealtime)) continue;
    if (!_readerFilterMatches(reader)) continue;
    if (apply !== 'immediate') apply.deferred.length = 0;
    rowId += 1n;
    const { ftsMatches, ftsNegative } = _scanRowData(
      reader, query, accumulator, rowId, apply, stats, needsFts,
    );
    const sourceRealtime = apply === 'immediate' ? null : _pickSourceRealtime(apply.deferred);
    const effective = _effectiveRealtimeFromScan(sourceRealtime, commitRealtime);
    _recordSourceRealtimeDelta(stats, sourceRealtime, commitRealtime);
    if (!_timestampInRange(query, effective)) continue;
    if (_rowRejectedByFts(query, ftsMatches, ftsNegative)) continue;
    _recordLastRealtime(stats, commitRealtime);
    stats.facetRowsMatched += 1n;
    if (apply !== 'immediate') {
      for (const [valueIndex] of apply.deferred) {
        accumulator.applyValue(valueIndex, null, stats);
      }
    }
    accumulator.finishFacetRow(rowId, stats);
  }
}

// ---------------------------------------------------------------------------
// Index strategy: derive candidate entry offsets via the FIELD chain,
// then count facets and histogram values without full row data.
// ---------------------------------------------------------------------------

function _exploreIndexed(reader, query, control) {
  _validateQuery(query);
  _validateIndexedQuery(query);
  const result = _explorerResultForQuery(reader, query);
  const candidates = _indexedCandidateSet(reader, query, null);
  if (control !== null && control._stopped) {
    _flushReaderFilters(reader);
    return result;
  }
  _indexedCollectRows(reader, query, result, candidates, control);
  if (control !== null && control._stopped) {
    _flushReaderFilters(reader);
    return result;
  }
  _indexedCollectFacets(reader, query, result, candidates, control);
  _indexedCollectHistogram(reader, query, result, candidates, control);
  _flushReaderFilters(reader);
  return result;
}

function _exploreCompare(reader, query) {
  const traversalStarted = Date.now();
  const traversal = _exploreTraversal(reader, query, null);
  const traversalDuration = (Date.now() - traversalStarted) / 1000;

  const indexStarted = Date.now();
  const indexed = _exploreIndexed(reader, query, null);
  const indexDuration = (Date.now() - indexStarted) / 1000;

  if (!_explorerOutputsMatch(traversal, indexed)) {
    throw new ExplorerError('indexed explorer output differs from traversal explorer output');
  }

  indexed.comparison = new ExplorerComparison();
  indexed.comparison.traversalDuration = traversalDuration;
  indexed.comparison.indexDuration = indexDuration;
  indexed.comparison.traversalStats = traversal.stats;
  indexed.comparison.indexStats = indexed.stats.copy();
  return indexed;
}

function _explorerOutputsMatch(left, right) {
  if (left.rows.length !== right.rows.length) return false;
  for (let i = 0; i < left.rows.length; i++) {
    const a = left.rows[i];
    const b = right.rows[i];
    if (a.realtimeUsec !== b.realtimeUsec || a.cursor !== b.cursor) return false;
    if (a.payloads.length !== b.payloads.length) return false;
    for (let j = 0; j < a.payloads.length; j++) {
      if (Buffer.isBuffer(a.payloads[j]) && Buffer.isBuffer(b.payloads[j])) {
        if (!a.payloads[j].equals(b.payloads[j])) return false;
      } else if (a.payloads[j] !== b.payloads[j]) return false;
    }
  }
  if (left.facets.size !== right.facets.size) return false;
  for (const [key, leftValues] of left.facets) {
    const rightValues = right.facets.get(key);
    if (rightValues === undefined) return false;
    if (leftValues.size !== rightValues.size) return false;
    for (const [vkey, lcount] of leftValues) {
      if (rightValues.get(vkey) !== lcount) return false;
    }
  }
  return _histogramsMatch(left.histogram, right.histogram);
}

function _histogramsMatch(left, right) {
  if (left === null && right === null) return true;
  if (left === null || right === null) return false;
  if (!left.field.equals(right.field) || left.buckets.length !== right.buckets.length) return false;
  for (let i = 0; i < left.buckets.length; i++) {
    const a = left.buckets[i];
    const b = right.buckets[i];
    if (a.startRealtimeUsec !== b.startRealtimeUsec || a.endRealtimeUsec !== b.endRealtimeUsec) return false;
    if (a.values.size !== b.values.size) return false;
    for (const [k, v] of a.values) {
      if (b.values.get(k) !== v) return false;
    }
  }
  return true;
}

function _indexedCandidateSet(reader, query, excludedField) {
  // Mirrors Rust indexed_candidate_set (L1489-1526). If no filters
  // and no time bound are active, every entry is a candidate.
  const hasActiveFilter = query.filters.some(
    (f) => (excludedField === null || !f.field.equals(excludedField)) && f.values.length > 0,
  );
  const hasTimeBound = query.afterRealtimeUsec !== null || query.beforeRealtimeUsec !== null;
  if (!hasActiveFilter && !hasTimeBound) {
    // All entries: replicate the n_entries count, no per-entry walk.
    return { all: true, count: Number(reader.header.n_entries) };
  }
  _configureFilters(reader, query, excludedField);
  _seekForExplorer(reader, query);
  const offsets = [];
  const seen = new Set();
  while (_stepExplorer(reader, query.direction)) {
    const commitRealtime = reader.getRealtimeUsec();
    if (commitRealtime === 0n) continue;
    if (_stopByCommitTime(query, commitRealtime)) break;
    if (_skipByCommitTime(query, commitRealtime)) continue;
    if (!_readerFilterMatches(reader)) continue;
    if (reader.entryIndex < 0 || reader.entryIndex >= reader.entryOffsets.length) continue;
    const entryOffset = reader.entryOffsets[reader.entryIndex];
    if (!seen.has(entryOffset)) {
      seen.add(entryOffset);
      offsets.push(entryOffset);
    }
  }
  _flushReaderFilters(reader);
  return { all: false, count: offsets.length, offsets };
}

function _indexedCollectRows(reader, query, result, candidates, control) {
  if (query.limit === 0) return;
  const candidateSet = candidates.all ? null : new Set(candidates.offsets);
  const sequence = candidates.all
    ? reader.entryOffsets.slice()
    : candidates.offsets.slice();
  const indexByOffset = new Map();
  for (let i = 0; i < reader.entryOffsets.length; i++) {
    indexByOffset.set(reader.entryOffsets[i], i);
  }
  if (query.direction === Direction.Backward) sequence.reverse();
  for (const entryOffset of sequence) {
    if (control !== null && control._stopped) break;
    if (candidateSet !== null && !candidateSet.has(entryOffset)) continue;
    const index = indexByOffset.get(entryOffset);
    if (index === undefined) continue;
    reader.entryIndex = index;
    reader._resetCachedEntryDataState();
    const commitRealtime = reader.getRealtimeUsec();
    if (commitRealtime === 0n) continue;
    if (_stopByCommitTime(query, commitRealtime)) return;
    if (_skipByCommitTime(query, commitRealtime)) continue;
    if (!_timestampInRange(query, commitRealtime)) continue;
    _recordLastRealtime(result.stats, commitRealtime);
    result.stats.rowsMatched += 1n;
    if (control !== null && control.emitMatchedRow(commitRealtime, result.stats.rowsMatched)) break;
    if (_rowWithinAnchor(query, commitRealtime) && result.rows.length < query.limit) {
      result.rows.push(_currentExplorerRow(reader, commitRealtime, result.stats, true));
    }
  }
  if (query.direction === Direction.Backward) {
    result.rows.sort((a, b) => (a.realtimeUsec < b.realtimeUsec ? 1 : a.realtimeUsec > b.realtimeUsec ? -1 : 0));
  } else {
    result.rows.sort((a, b) => (a.realtimeUsec < b.realtimeUsec ? -1 : a.realtimeUsec > b.realtimeUsec ? 1 : 0));
  }
  if (result.rows.length > query.limit) result.rows.length = query.limit;
}

function _indexedCollectFacets(reader, query, result, candidates, control) {
  if (control !== null && control._stopped) return;
  for (const group of _facetPassGroups(query)) {
    let groupCandidates = candidates;
    if (group.excludedField !== null) {
      groupCandidates = _indexedCandidateSet(reader, query, group.excludedField);
    }
    result.stats.facetRowsMatched += BigInt(groupCandidates.count);
    for (const facetIndex of group.facetIndices) {
      if (facetIndex >= query.facets.length) continue;
      const field = query.facets[facetIndex];
      _indexedCountFacetField(reader, field, groupCandidates, result, query, control);
      if (control !== null && control._stopped) return;
    }
  }
}

function _indexedCollectHistogram(reader, query, result, candidates, control) {
  if (query.histogram === null) return;
  if (control !== null && control._stopped) return;
  _indexedCountHistogram(reader, query.histogram, candidates, result, query, control);
}

function _indexedCountFacetField(reader, field, candidates, result, query, control) {
  const values = new Map();
  const rowsWithField = new Set();
  const candidateSet = candidates.all ? null : new Set(candidates.offsets);
  let fieldOffset = reader._findFieldHeadDataOffset(field);
  while (fieldOffset !== 0n) {
    if (control !== null && control._stopped) break;
    let dataHeader;
    let payload;
    try {
      dataHeader = reader._readDataHeaderAt(fieldOffset);
      payload = reader._readDataPayloadAt(fieldOffset, false);
    } catch {
      break;
    }
    result.stats.dataObjectsClassified += 1n;
    result.stats.dataPayloadsLoaded += 1n;
    if (_dataObjectWasCompressed(reader, fieldOffset)) {
      result.stats.payloadsDecompressed += 1n;
    }
    const split = _splitPayload(payload);
    if (split === null || !split[0].equals(field)) {
      fieldOffset = dataHeader.nextFieldOffset;
      continue;
    }
    const value = split[1];
    let count = 0n;
    for (const entryOffset of _dataEntryOffsets(reader, dataHeader)) {
      result.stats.dataRefsSeen += 1n;
      if (candidateSet === null || candidateSet.has(entryOffset)) {
        count += 1n;
        rowsWithField.add(entryOffset);
      }
    }
    if (count !== 0n) {
      _incrementCounter(values, value, count);
      result.stats.facetUpdates += count;
    }
    fieldOffset = dataHeader.nextFieldOffset;
  }
  const unset = BigInt(candidates.count) - BigInt(rowsWithField.size);
  if (unset !== 0n) {
    _incrementCounter(values, UNSET_VALUE, unset);
    result.stats.facetUpdates += unset;
  }
  result.facets.set(field.toString('hex'), values);
}

function _indexedCountHistogram(reader, field, candidates, result, query, control) {
  const histogram = result.histogram;
  if (histogram === null || histogram.buckets.length === 0) return;
  const histogramStart = histogram.buckets[0].startRealtimeUsec;
  let width = histogram.buckets[0].endRealtimeUsec - histogram.buckets[0].startRealtimeUsec;
  if (width <= 0n) width = 1n;
  const bucketCount = histogram.buckets.length;
  const rowsWithField = new Set();
  const candidateSet = candidates.all ? null : new Set(candidates.offsets);
  let fieldOffset = reader._findFieldHeadDataOffset(field);
  while (fieldOffset !== 0n) {
    if (control !== null && control._stopped) break;
    let dataHeader;
    let payload;
    try {
      dataHeader = reader._readDataHeaderAt(fieldOffset);
      payload = reader._readDataPayloadAt(fieldOffset, false);
    } catch {
      break;
    }
    result.stats.dataObjectsClassified += 1n;
    result.stats.dataPayloadsLoaded += 1n;
    if (_dataObjectWasCompressed(reader, fieldOffset)) {
      result.stats.payloadsDecompressed += 1n;
    }
    const split = _splitPayload(payload);
    if (split === null || !split[0].equals(field)) {
      fieldOffset = dataHeader.nextFieldOffset;
      continue;
    }
    const value = split[1];
    for (const entryOffset of _dataEntryOffsets(reader, dataHeader)) {
      result.stats.dataRefsSeen += 1n;
      if (candidateSet !== null && !candidateSet.has(entryOffset)) continue;
      rowsWithField.add(entryOffset);
      const commitRealtime = reader._entryRealtimeAtOffset(entryOffset);
      if (commitRealtime === 0n) continue;
      if (!_timestampInRange(query, commitRealtime)) continue;
      const idx = _histogramBucketIndexFromBounds(
        commitRealtime, histogramStart, width, bucketCount,
      );
      if (idx === null) continue;
      const bucket = histogram.buckets[idx];
      _incrementCounter(bucket.values, value, 1n);
      result.stats.histogramUpdates += 1n;
    }
    fieldOffset = dataHeader.nextFieldOffset;
  }
  const sequence = candidates.all ? reader.entryOffsets.slice() : candidates.offsets.slice();
  for (const entryOffset of sequence) {
    if (rowsWithField.has(entryOffset)) continue;
    const commitRealtime = reader._entryRealtimeAtOffset(entryOffset);
    if (commitRealtime === 0n) continue;
    if (!_timestampInRange(query, commitRealtime)) continue;
    const idx = _histogramBucketIndexFromBounds(
      commitRealtime, histogramStart, width, bucketCount,
    );
    if (idx === null) continue;
    const bucket = histogram.buckets[idx];
    _incrementCounter(bucket.values, UNSET_VALUE, 1n);
    result.stats.histogramUpdates += 1n;
  }
}

function _dataEntryOffsets(reader, dataHeader) {
  // Mirrors Python's _data_entry_offsets; yields every entry offset a
  // DATA object references via entry_offset and entry_array_offset chain.
  let nEntries = Number(dataHeader.nEntries);
  if (nEntries === 0) return [];
  const out = [];
  const firstEntry = dataHeader.entryOffset;
  if (firstEntry !== 0n) {
    out.push(firstEntry);
    nEntries -= 1;
  }
  let arrayOffset = dataHeader.entryArrayOffset;
  while (arrayOffset !== 0n && nEntries > 0) {
    let array;
    try {
      array = reader._readEntryArrayObject(arrayOffset);
    } catch {
      return out;
    }
    if (array === null || array === undefined) return out;
    const dataStart = Number(array.dataStart);
    const itemSize = array.itemSize;
    const take = Math.min(nEntries, array.capacity);
    for (let i = 0; i < take; i++) {
      const off = reader._readEntryArrayItemOffset(dataStart + i * itemSize);
      if (off !== 0n) out.push(off);
    }
    nEntries -= take;
    arrayOffset = array.nextOffset;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Public entry points consumed by FileReader.explore / .exploreWithStrategy /
// .exploreWithStrategyAndControl. These live in the module-private layer
// so the reader file stays focused on binary IO.
// ---------------------------------------------------------------------------

export function exploreWithStrategy(reader, query, strategy) {
  let s = strategy;
  if (s === ExplorerStrategy.Traversal) s = 'traversal';
  else if (s === ExplorerStrategy.Index) s = 'index';
  else if (s === ExplorerStrategy.Compare) s = 'compare';
  return _exploreFileReader(reader, query, s, null);
}

export function exploreWithStrategyAndControl(reader, query, strategy, control) {
  let s = strategy;
  if (s === ExplorerStrategy.Traversal) s = 'traversal';
  else if (s === ExplorerStrategy.Index) s = 'index';
  else if (s === ExplorerStrategy.Compare) s = 'compare';
  return _exploreFileReader(reader, query, s, control);
}
