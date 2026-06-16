// Single journal file reader.
// Reads .journal, .journal~, .journal.zst, .journal~.zst files.
// Uses entry-array-based iteration (matching Go/Rust).

import { TextDecoder } from 'node:util';
import { uuidToString } from './binary.js';
import {
  parseFileHeader, parseObjectHeader,
  HEADER_MIN_SIZE, HEADER_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
  OBJECT_TYPE_DATA, OBJECT_TYPE_FIELD, OBJECT_HEADER_SIZE,
  DATA_OBJECT_HEADER_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE, INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPACT,
  COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
  INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_LZ4,
  COMPACT_ENTRY_ITEM_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE,
  FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE,
  ENTRY_OBJECT_HEADER_SIZE, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
} from './header.js';
import { decompressZstdDataPayload, isZstFile } from './compress.js';
import { decompressLz4DataPayload } from './lz4-block.js';
import { decompressXzDataPayload } from './xz-block.js';
import { jenkinsHash64, sipHash24 } from './hash.js';
import { exploreWithStrategy, exploreWithStrategyAndControl } from './explorer.js';
import {
  openReaderAccessor,
  normalizeReaderOptions,
  withSnapshotBounds,
  READER_BOUNDS_SNAPSHOT,
} from './reader-access.js';
import { streamZstToTempSync } from './zst-stream.js';

const utf8Decoder = new TextDecoder('utf-8', { fatal: true });
const SUPPORTED_INCOMPATIBLE_FLAGS = INCOMPATIBLE_KEYED_HASH |
  INCOMPATIBLE_COMPRESSED_XZ |
  INCOMPATIBLE_COMPRESSED_ZSTD |
  INCOMPATIBLE_COMPRESSED_LZ4 |
  INCOMPATIBLE_COMPACT;
const OBJECT_COMPRESSED_MASK = OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD;

const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

function getOwn(object, key) {
  return hasOwn(object, key) ? Reflect.get(object, key) : undefined;
}

function setOwn(object, key, value) {
  Reflect.set(object, key, value);
}

function pushOwnArray(object, key, value) {
  const values = getOwn(object, key);
  if (values) {
    values.push(value);
    return;
  }
  setOwn(object, key, [value]);
}

export class FileReader {
  constructor(accessor, header, path, cleanup) {
    this.accessor = accessor;
    this.header = header;
    this.path = path;
    this.cleanup = cleanup;

    this.entryOffsets = [];
    this.entryIndex = -1;
    this.direction = 0;
    this.filter = null;
    this.realtimeSeek = null;
    this.entryDataOffsets = [];
    this.entryDataOffsetsEntry = null;
    this.entryDataIndex = 0;
    this.entryDataStateActive = false;

    this.compact = this._headerIsCompact();
    this.entryItemSize = this.compact ? COMPACT_ENTRY_ITEM_SIZE : REGULAR_ENTRY_ITEM_SIZE;
    this.offsetArrayItemSizeValue = this.compact ? COMPACT_OFFSET_ARRAY_ITEM_SIZE : REGULAR_OFFSET_ARRAY_ITEM_SIZE;
    this.dataPayloadOffsetValue = this.compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;

    this._loadEntryArray();
  }

  // Open a journal file, decompressing .zst if needed.
  static open(path, options = {}) {
    const readerOptions = normalizeReaderOptions(options);
    let accessor = null;
    let temp = null;

    try {
      let openPath = path;
      let openOptions = readerOptions;
      if (isZstFile(path)) {
        temp = streamZstToTempSync(path, {
          prefix: 'node-sdk-journal',
          timeoutMs: readerOptions.zstdTimeoutMs,
        });
        openPath = temp.path;
        openOptions = withSnapshotBounds(readerOptions);
      }

      accessor = openReaderAccessor(openPath, openOptions);
      if (accessor.size() < HEADER_MIN_SIZE) {
        throw new Error('file too small for journal header');
      }

      const header = readHeaderFromAccessor(accessor);

      ensureSupportedHeader(header);

      return new FileReader(accessor, header, path, temp?.cleanup ?? null);
    } catch (err) {
      try { accessor?.close(); } catch {
        // Best-effort cleanup while preserving the original open failure.
      }
      try { temp?.cleanup?.(); } catch {
        // Best-effort cleanup while preserving the original open failure.
      }
      throw err;
    }
  }

  // Load all entry offsets from the entry array chain.
  _loadEntryArray() {
    this.entryOffsets = this._readEntryArrayOffsets();
    this.entryIndex = -1;
  }

  _readEntryArrayOffsets() {
    if (this.header.entry_array_offset === 0n) {
      return [];
    }

    const state = {
      offsets: [],
      offset: this.header.entry_array_offset,
      remaining: this.header.n_entries,
    };
    const visited = new Set();

    while (state.offset !== 0n && state.remaining > 0n) {
      const visitKey = state.offset.toString();
      if (visited.has(visitKey)) {
        throw new Error(`entry array chain cycle at offset ${state.offset}`);
      }
      visited.add(visitKey);
      const segment = this._readEntryArraySegment(state.offset, state.remaining);
      if (!segment) break;
      if (segment.toRead <= 0) {
        throw new Error(`entry array chain made no progress at offset ${state.offset}`);
      }
      this._appendEntryArraySegmentOffsets(state.offsets, segment);
      state.remaining -= BigInt(segment.toRead);
      state.offset = segment.nextOffset;
    }

    return state.offsets;
  }

  _readEntryArraySegment(offset, remaining) {
    const oh = this._readObjectHeaderAt(offset);
    if (!oh || oh.type !== OBJECT_TYPE_ENTRY_ARRAY) return null;
    const objSize = oh.size;
    if (objSize < BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) return null;
    const itemSize = this.offsetArrayItemSizeValue;
    const payloadSize = objSize - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE);
    if (payloadSize % BigInt(itemSize) !== 0n) {
      throw new Error('entry array item payload has invalid compact alignment');
    }
    const capacity = Number(payloadSize / BigInt(itemSize));
    return {
      dataStart: Number(offset) + OFFSET_ARRAY_OBJECT_HEADER_SIZE,
      itemSize,
      nextOffset: this._u64(Number(offset) + 16),
      toRead: Number(remaining < BigInt(capacity) ? remaining : BigInt(capacity)),
    };
  }

  _appendEntryArraySegmentOffsets(offsets, segment) {
    for (let i = 0; i < segment.toRead; i++) {
      const itemOffset = segment.dataStart + i * segment.itemSize;
      const entryOff = this._isCompact()
        ? BigInt(this._u32(itemOffset))
        : this._u64(itemOffset);
      if (entryOff !== 0n && this._validEntryOffset(entryOff)) offsets.push(entryOff);
    }
  }

  refresh() {
    return this._refreshEntryOffsets();
  }

  _refreshEntryOffsets() {
    if (this.cleanup) return false;
    if (this.accessor.stats().bounds === READER_BOUNDS_SNAPSHOT) return false;

    const oldState = this._readerStateSnapshot();
    const snapshot = this._readRefreshSnapshot();
    if (!snapshot) return false;

    if (!this._refreshSnapshotChanged(snapshot)) {
      this.header = snapshot.header;
      this.entryIndex = Math.min(this.entryIndex, this.entryOffsets.length);
      return false;
    }

    return this._reloadEntryOffsetsFromDisk(oldState);
  }

  _readRefreshSnapshot() {
    const oldBounds = this.accessor.snapshotVisibleBounds();
    try {
      this.accessor.refreshVisibleBounds();
      if (this.accessor.size() <= 0) return null;
      return { oldSize: oldBounds, size: this.accessor.size(), header: this._readCurrentHeader() };
    } catch {
      this.accessor.restoreVisibleBounds(oldBounds);
      return null;
    }
  }

  _refreshSnapshotChanged(snapshot) {
    return snapshot.size !== snapshot.oldSize ||
      snapshot.header.n_entries !== this.header.n_entries ||
      snapshot.header.tail_entry_array_offset !== this.header.tail_entry_array_offset ||
      snapshot.header.tail_entry_array_n_entries !== this.header.tail_entry_array_n_entries;
  }

  _readerStateSnapshot() {
    return {
      header: this.header,
      offsets: this.entryOffsets,
      index: this.entryIndex,
      visibleBounds: this.accessor.snapshotVisibleBounds(),
      compact: this.compact,
      entryItemSize: this.entryItemSize,
      offsetArrayItemSize: this.offsetArrayItemSizeValue,
      dataPayloadOffset: this.dataPayloadOffsetValue,
      entryDataOffsets: this.entryDataOffsets,
      entryDataOffsetsEntry: this.entryDataOffsetsEntry,
      entryDataIndex: this.entryDataIndex,
      entryDataStateActive: this.entryDataStateActive,
    };
  }

  _reloadEntryOffsetsFromDisk(oldState = this._readerStateSnapshot()) {
    try {
      if (this.accessor.size() < HEADER_MIN_SIZE) throw new Error('file too small for journal header');
      const header = this._readCurrentHeader();
      ensureSupportedHeader(header);
      this.header = header;
      this._updateLayoutCache();
      this.entryOffsets = this._readEntryArrayOffsets();
      this.entryIndex = Math.min(oldState.index, this.entryOffsets.length);
      this.entryDataOffsets = oldState.entryDataOffsets;
      this.entryDataOffsetsEntry = oldState.entryDataOffsetsEntry;
      this.entryDataIndex = oldState.entryDataIndex;
      this.entryDataStateActive = oldState.entryDataStateActive;
      return (
        this.entryOffsets.length !== oldState.offsets.length ||
        (
          this.entryOffsets.length > 0 &&
          oldState.offsets.length > 0 &&
          this.entryOffsets[this.entryOffsets.length - 1] !== oldState.offsets[oldState.offsets.length - 1]
        )
      );
    } catch {
      this._restoreReaderState(oldState);
      return false;
    }
  }

  _restoreReaderState(state) {
    this.accessor.restoreVisibleBounds(state.visibleBounds);
    this.header = state.header;
    this.entryOffsets = state.offsets;
    this.entryIndex = Math.min(state.index, this.entryOffsets.length);
    this.compact = state.compact;
    this.entryItemSize = state.entryItemSize;
    this.offsetArrayItemSizeValue = state.offsetArrayItemSize;
    this.dataPayloadOffsetValue = state.dataPayloadOffset;
    this.entryDataOffsets = state.entryDataOffsets;
    this.entryDataOffsetsEntry = state.entryDataOffsetsEntry;
    this.entryDataIndex = state.entryDataIndex;
    this.entryDataStateActive = state.entryDataStateActive;
  }

  _readCurrentHeader() {
    const header = readHeaderFromAccessor(this.accessor);
    ensureSupportedHeader(header);
    return header;
  }

  _validEntryOffset(offset) {
    const off = Number(offset);
    if (off + OBJECT_HEADER_SIZE > this._visibleSize()) return false;
    const oh = this._readObjectHeaderAt(off);
    if (!oh) return false;
    if (oh.type === 0 && oh.size === 0n) return false;
    if (oh.type !== OBJECT_TYPE_ENTRY) return false;
    if (BigInt(off) + oh.size > BigInt(this._visibleSize())) return false;
    return true;
  }

  seekHead() {
    this._clearRowForPositionChange();
    this.entryIndex = -1;
    this.direction = 0;
    this.realtimeSeek = null;
  }

  seekTail() {
    this._clearRowForPositionChange();
    this.entryIndex = this.entryOffsets.length;
    this.direction = 1;
    this.realtimeSeek = null;
  }

  seekRealtimeUsec(usec) {
    this._clearRowForPositionChange();
    this.realtimeSeek = BigInt(usec);
  }

  next() {
    this._clearRowForPositionChange();
    if (this.realtimeSeek !== null) {
      const idx = this._firstRealtimeIndexAtOrAfter(this.realtimeSeek);
      let effectiveIdx = idx;
      if (effectiveIdx >= this.entryOffsets.length && this._refreshEntryOffsets()) {
        effectiveIdx = this._firstRealtimeIndexAtOrAfter(this.realtimeSeek);
      }
      this.realtimeSeek = null;
      this.direction = 0;
      if (effectiveIdx >= this.entryOffsets.length) {
        this.entryIndex = this.entryOffsets.length;
        return false;
      }
      this.entryIndex = effectiveIdx;
      return true;
    }
    this.direction = 0;
    if (this.entryIndex >= this.entryOffsets.length) {
      const nextIndex = this.entryIndex;
      if (this._refreshEntryOffsets() && nextIndex < this.entryOffsets.length) {
        this.entryIndex = nextIndex;
        return true;
      }
      this.entryIndex = this.entryOffsets.length;
      return false;
    }
    this.entryIndex++;
    if (this.entryIndex >= this.entryOffsets.length) {
      const nextIndex = this.entryIndex;
      if (this._refreshEntryOffsets() && nextIndex < this.entryOffsets.length) {
        this.entryIndex = nextIndex;
        return true;
      }
      this.entryIndex = this.entryOffsets.length;
      return false;
    }
    return true;
  }

  previous() {
    this._clearRowForPositionChange();
    if (this.realtimeSeek !== null) {
      const idx = this._lastRealtimeIndexAtOrBefore(this.realtimeSeek);
      this.realtimeSeek = null;
      this.direction = 1;
      if (idx < 0) {
        this.entryIndex = -1;
        return false;
      }
      this.entryIndex = idx;
      return true;
    }
    this.direction = 1;
    this.entryIndex--;
    if (this.entryIndex < 0) {
      this.entryIndex = -1;
      return false;
    }
    return true;
  }

  _firstRealtimeIndexAtOrAfter(usec) {
    let lo = 0;
    let hi = this.entryOffsets.length;
    while (lo < hi) {
      const mid = Math.floor((lo + hi) / 2);
      if (this._entryRealtimeAtIndex(mid) >= usec) hi = mid;
      else lo = mid + 1;
    }
    return lo;
  }

  _lastRealtimeIndexAtOrBefore(usec) {
    let lo = 0;
    let hi = this.entryOffsets.length;
    while (lo < hi) {
      const mid = Math.floor((lo + hi) / 2);
      if (this._entryRealtimeAtIndex(mid) > usec) hi = mid;
      else lo = mid + 1;
    }
    return lo - 1;
  }

  _entryRealtimeAtIndex(index) {
    const offset = this.entryOffsets.at(index);
    return this._u64(Number(offset) + OBJECT_HEADER_SIZE + 8);
  }

  step() {
    for (;;) {
      if (!this.next()) return false;
      if (!this.filter) return true;
      let entry;
      try {
        entry = this.getEntry();
      } catch { /* skip corrupt */ }
      if (entry && this.filter.matches(entry)) return true;
    }
  }

  stepBack() {
    for (;;) {
      if (!this.previous()) return false;
      if (!this.filter) return true;
      let entry;
      try {
        entry = this.getEntry();
      } catch { /* skip corrupt */ }
      if (entry && this.filter.matches(entry)) return true;
    }
  }

  getEntry() {
    this._invalidateEntryDataState();
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) return null;
    return this._readEntryAt(this.entryOffsets[this.entryIndex]);
  }

  _readEntryAt(offset) {
    const { entry: e, dataOffsets } = this._readEntryMetadataAndOffsets(offset);

    const fields = Object.create(null);
    const fieldValues = Object.create(null);
    const rawFieldValues = new Map();
    const rawFields = [];
    const payloads = [];

    for (const dataOffset of dataOffsets) {
      try {
        const payload = this._readDataPayloadAt(dataOffset);
        const eqPos = payload.indexOf(0x3d);
        if (eqPos < 0) continue;
        const name = Buffer.from(payload.slice(0, eqPos));
        const valueBuf = Buffer.from(payload.slice(eqPos + 1));
        const payloadBuf = Buffer.from(payload);
        payloads.push(payloadBuf);
        rawFields.push([name, valueBuf]);
        const rawKey = fieldKey(name);
        const rawValues = rawFieldValues.get(rawKey);
        if (rawValues) rawValues.push(valueBuf);
        else rawFieldValues.set(rawKey, [valueBuf]);

        const nameStr = decodeUtf8OrNull(name);
        if (nameStr === null) continue;
        if (!hasOwn(fields, nameStr)) setOwn(fields, nameStr, valueBuf);
        pushOwnArray(fieldValues, nameStr, valueBuf);
      } catch { /* skip corrupt data */ }
    }

    const cursor = this._makeCursor(offset, e);
    return {
      fields,
      fieldValues,
      rawFields,
      rawFieldValues,
      payloads,
      seqnum: e.seqnum,
      realtime: e.realtime,
      monotonic: e.monotonic,
      boot_id: e.boot_id,
      xor_hash: e.xor_hash,
      cursor,
    };
  }

  _makeCursor(entryOffset, e) {
    const seqnumId = uuidToString(this.header.seqnum_id);
    const bootId = uuidToString(e.boot_id);
    const realtimeHex = e.realtime.toString(16).padStart(16, '0');
    return `s=${seqnumId};j=${bootId};c=${realtimeHex};n=${e.seqnum}`;
  }

  getRealtimeUsec() {
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) return 0n;
    const offset = this.entryOffsets[this.entryIndex];
    return this._u64(Number(offset) + OBJECT_HEADER_SIZE + 8);
  }

  getCursor() {
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) return null;
    const offset = this.entryOffsets[this.entryIndex];
    const { entry: e } = this._readEntryMetadataAndOffsets(offset, false);
    return this._makeCursor(offset, e);
  }

  testCursor(cursor) { return this.getCursor() === cursor; }

  addMatch(data) {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addMatch(data);
  }

  addDisjunction() {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addDisjunction();
  }

  addConjunction() {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addConjunction();
  }

  flushMatches() { this.filter = null; }

  queryUnique(fieldName) {
    const field = fieldNameBytes(fieldName);
    const results = [];
    let offset = this._findFieldHeadDataOffset(field);
    while (offset !== 0n) {
      const data = this._readDataHeaderAt(offset);
      const payload = this._readDataPayloadAt(offset, false);
      if (
        payload.length <= field.length ||
        !payload.subarray(0, field.length).equals(field) ||
        payload[field.length] !== 0x3d
      ) {
        throw new Error(`field data object at offset ${offset} does not match requested field`);
      }
      const value = Buffer.from(payload.subarray(field.length + 1));
      results.push(value);
      offset = data.nextFieldOffset;
    }
    return results;
  }

  enumerateFields() {
    try {
      return this._enumerateFieldsIndexed();
    } catch {
      return this._enumerateFieldsByEntryScan();
    }
  }

  _enumerateFieldsIndexed() {
    const fields = new Set();
    const tableOffset = this.header.field_hash_table_offset;
    const tableSize = this.header.field_hash_table_size;
    if (tableOffset === 0n || tableSize < BigInt(HASH_ITEM_SIZE)) return this._enumerateFieldsByEntryScan();
    const buckets = tableSize / BigInt(HASH_ITEM_SIZE);
    for (let bucket = 0n; bucket < buckets; bucket++) {
      const bucketOffset = Number(tableOffset + bucket * BigInt(HASH_ITEM_SIZE));
      if (this._visibleSize() < bucketOffset + HASH_ITEM_SIZE) {
        throw new Error('field hash bucket exceeds buffer');
      }
      let offset = this._u64(bucketOffset);
      while (offset !== 0n) {
        const field = this._readFieldObjectAt(offset);
        const name = decodeUtf8OrNull(field.payload);
        if (name !== null) fields.add(name);
        offset = field.nextHashOffset;
      }
    }
    return fields;
  }

  _enumerateFieldsByEntryScan() {
    const fields = new Set();
    for (const off of this.entryOffsets) {
      try {
        const entry = this._readEntryAt(off);
        if (entry) for (const k of Object.keys(entry.fields)) fields.add(k);
      } catch {
        // Compatibility fallback skips corrupt entries that normal iteration cannot decode.
      }
    }
    return fields;
  }

  close() {
    this._clearRowForPositionChange();
    try { this.accessor?.close(); } finally {
      this.accessor = null;
      try { this.cleanup?.(); } catch {
        // Best-effort cleanup on reader close.
      }
      this.cleanup = null;
    }
  }

  // -------------------------------------------------------------------------
  // Explorer integration (SOW-0105 chunk 1).
  //
  // These methods exist solely so the explorer module can walk DATA-object
  // entry references (entry_offset + entry_array chain) when implementing
  // the indexed strategy. They mirror the Reader shim that
  // python/journal/explorer.py's _data_entry_offsets uses, which in turn
  // mirrors the Rust FileReader::field_data_objects_with_offsets visitor
  // in rust/src/journal/src/explorer.rs (L2294-2394).
  //
  // The two helpers are not part of the public API; they are private and
  // only used by ./explorer.js. We do not expose them on DirectoryReader
  // because Rust has no directory-level Explorer and SOW-0105 chunk 1
  // delivers single-file Explorer only.
  // -------------------------------------------------------------------------

  _readEntryArrayObject(offset) {
    const pos = Number(offset);
    if (this._visibleSize() < pos + OFFSET_ARRAY_OBJECT_HEADER_SIZE) {
      throw new Error('buffer too small for entry array object');
    }
    const oh = this._readObjectHeaderAt(pos);
    if (!oh || oh.type !== OBJECT_TYPE_ENTRY_ARRAY) {
      throw new Error('corrupt ENTRY_ARRAY object');
    }
    const itemSize = this.offsetArrayItemSizeValue;
    const payloadSize = oh.size - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE);
    if (payloadSize < 0n || payloadSize % BigInt(itemSize) !== 0n) {
      throw new Error('entry array payload has invalid alignment');
    }
    const capacity = Number(payloadSize / BigInt(itemSize));
    return {
      dataStart: pos + OFFSET_ARRAY_OBJECT_HEADER_SIZE,
      itemSize,
      capacity,
      nextOffset: this._u64(pos + 16),
    };
  }

  _readEntryArrayItemOffset(byteOffset) {
    if (this._isCompact()) {
      const v = this._u32(byteOffset);
      return v === 0 ? 0n : BigInt(v);
    }
    return this._u64(byteOffset);
  }

  _entryRealtimeAtOffset(offset) {
    return this._u64(Number(offset) + OBJECT_HEADER_SIZE + 8);
  }

  // Public Explorer entry points. Implemented in ./explorer.js to keep
  // this reader file focused on binary IO.
  explore(query) {
    return exploreWithStrategy(this, query, 'traversal');
  }

  exploreWithStrategy(query, strategy) {
    return exploreWithStrategy(this, query, strategy);
  }

  exploreWithStrategyAndControl(query, strategy, control) {
    return exploreWithStrategyAndControl(this, query, strategy, control);
  }

  _isCompact() {
    return this.compact;
  }

  _offsetArrayItemSize() {
    return this.offsetArrayItemSizeValue;
  }

  _dataPayloadOffset() {
    return this.dataPayloadOffsetValue;
  }

  _headerIsCompact() {
    return (this.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;
  }

  _updateLayoutCache() {
    this.compact = this._headerIsCompact();
    this.entryItemSize = this.compact ? COMPACT_ENTRY_ITEM_SIZE : REGULAR_ENTRY_ITEM_SIZE;
    this.offsetArrayItemSizeValue = this.compact ? COMPACT_OFFSET_ARRAY_ITEM_SIZE : REGULAR_OFFSET_ARRAY_ITEM_SIZE;
    this.dataPayloadOffsetValue = this.compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
  }

  currentEntryKey() {
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) return null;
    const offset = this.entryOffsets[this.entryIndex];
    const { entry } = this._readEntryMetadataAndOffsets(offset, false);
    return {
      seqnumId: Buffer.from(this.header.seqnum_id),
      seqnum: entry.seqnum,
      bootId: Buffer.from(entry.boot_id),
      monotonic: entry.monotonic,
      realtime: entry.realtime,
      xorHash: entry.xor_hash,
    };
  }

  visitEntryPayloads(visitor) {
    this._invalidateEntryDataState();
    const offsets = this._currentEntryDataOffsets();
    for (const dataOffset of offsets) {
      visitor(this._readDataPayloadAt(dataOffset));
    }
  }

  collectEntryPayloads() {
    const payloads = [];
    this.visitEntryPayloads((payload) => payloads.push(payload));
    return payloads;
  }

  getEntryPayload(fieldName) {
    const prefix = Buffer.concat([fieldNameBytes(fieldName), Buffer.from('=')]);
    let found = null;
    this.visitEntryPayloads((payload) => {
      if (found === null && payload.subarray(0, prefix.length).equals(prefix)) {
        found = payload;
      }
    });
    return found;
  }

  getRaw(fieldName) {
    const values = this.getRawValues(fieldName);
    return values.length > 0 ? values[0] : null;
  }

  getRawValues(fieldName) {
    const entry = this.getEntry();
    if (!entry) return [];
    return Array.from(entry.rawFieldValues.get(fieldKey(fieldNameBytes(fieldName))) || []);
  }

  entryDataRestart() {
    this.entryDataOffsets = this._currentEntryDataOffsets();
    this.entryDataIndex = 0;
    this.entryDataStateActive = true;
  }

  enumerateEntryPayload() {
    if (this.entryDataIndex >= this.entryDataOffsets.length) {
      this.clearEntryDataState();
      return null;
    }
    const dataOffset = this.entryDataOffsets[this.entryDataIndex++];
    this.entryDataStateActive = true;
    return this._readDataPayloadAt(dataOffset);
  }

  clearEntryDataState() {
    this._resetCachedEntryDataState();
  }

  accessStats() {
    return this.accessor?.stats() ?? {};
  }

  _clearRowForPositionChange() {
    this._resetCachedEntryDataState();
    this.accessor?.clearRow();
  }

  _resetCachedEntryDataState() {
    this.entryDataOffsets = [];
    this.entryDataOffsetsEntry = null;
    this.entryDataIndex = 0;
    this.entryDataStateActive = false;
  }

  _invalidateEntryDataState() {
    if (this.entryDataStateActive) this.clearEntryDataState();
  }

  _currentEntryDataOffsets() {
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) {
      throw new Error('no entry at current position');
    }
    const entryOffset = this.entryOffsets[this.entryIndex];
    if (this.entryDataOffsetsEntry !== entryOffset) {
      const { dataOffsets } = this._readEntryMetadataAndOffsets(entryOffset);
      this.entryDataOffsets = dataOffsets;
      this.entryDataOffsetsEntry = entryOffset;
    }
    return this.entryDataOffsets;
  }

  _readEntryMetadataAndOffsets(offset, includeOffsets = true) {
    const e = this._readEntryObjectAt(offset);
    return {
      entry: e,
      dataOffsets: includeOffsets ? e.items.map((item) => item.offset).filter((itemOffset) => itemOffset !== 0n) : [],
    };
  }

  _readEntryObjectAt(offset) {
    const position = Number(offset);
    const oh = this._readObjectHeaderAt(position);
    if (!oh || oh.type !== OBJECT_TYPE_ENTRY) {
      throw new Error(`expected ENTRY (type ${OBJECT_TYPE_ENTRY}), got type ${oh?.type} at offset ${position}`);
    }
    if (oh.size < BigInt(ENTRY_OBJECT_HEADER_SIZE)) {
      throw new Error(`entry object too small: ${oh.size}`);
    }
    if (BigInt(position) + oh.size > BigInt(this._visibleSize())) {
      throw new Error(`entry object exceeds buffer at offset ${position}`);
    }
    const itemSize = this.compact ? COMPACT_ENTRY_ITEM_SIZE : REGULAR_ENTRY_ITEM_SIZE;
    if ((oh.size - BigInt(ENTRY_OBJECT_HEADER_SIZE)) % BigInt(itemSize) !== 0n) {
      throw new Error(`entry object item payload is not ${itemSize}-byte aligned`);
    }
    const items = [];
    const nItems = Number((oh.size - BigInt(ENTRY_OBJECT_HEADER_SIZE)) / BigInt(itemSize));
    const itemsStart = position + ENTRY_OBJECT_HEADER_SIZE;
    for (let i = 0; i < nItems; i++) {
      const itemOffset = itemsStart + i * itemSize;
      const dataOffset = this.compact ? BigInt(this._u32(itemOffset)) : this._u64(itemOffset);
      const dataHash = this.compact ? 0n : this._u64(itemOffset + 8);
      if (dataOffset !== 0n) items.push({ offset: dataOffset, hash: dataHash });
    }
    return {
      seqnum: this._u64(position + OBJECT_HEADER_SIZE),
      realtime: this._u64(position + OBJECT_HEADER_SIZE + 8),
      monotonic: this._u64(position + OBJECT_HEADER_SIZE + 16),
      boot_id: this._readBytes(position + OBJECT_HEADER_SIZE + 24, 16),
      xor_hash: this._u64(position + OBJECT_HEADER_SIZE + 40),
      items,
    };
  }

  _readDataPayloadAt(offset, rowLifetime = true) {
    const position = Number(offset);
    const oh = this._readObjectHeaderAt(position);
    if (!oh || oh.type !== OBJECT_TYPE_DATA || oh.size < BigInt(this.dataPayloadOffsetValue)) {
      throw new Error('corrupt DATA object');
    }
    if (BigInt(position) + oh.size > BigInt(this._visibleSize())) {
      throw new Error(`data object exceeds buffer at offset ${offset}`);
    }
    const compressionFlags = oh.flags & OBJECT_COMPRESSED_MASK;
    if ((oh.flags & ~OBJECT_COMPRESSED_MASK) !== 0) {
      throw new Error(`unsupported DATA object flags: 0x${oh.flags.toString(16)}`);
    }
    if (compressionFlags !== 0 && (compressionFlags & (compressionFlags - 1)) !== 0) {
      throw new Error(`unsupported DATA object compression flags: 0x${oh.flags.toString(16)}`);
    }
    const payloadOffset = position + this.dataPayloadOffsetValue;
    const payloadSize = Number(oh.size) - this.dataPayloadOffsetValue;
    if (compressionFlags === 0) {
      return rowLifetime
        ? this.accessor.rowView(payloadOffset, payloadSize)
        : this.accessor.tempView(payloadOffset, payloadSize);
    }
    const compressed = this.accessor.tempView(payloadOffset, payloadSize);
    let payload;
    if (oh.flags & OBJECT_COMPRESSED_LZ4) payload = decompressLz4DataPayload(compressed);
    else if (oh.flags & OBJECT_COMPRESSED_ZSTD) payload = decompressZstdDataPayload(compressed);
    else if (oh.flags & OBJECT_COMPRESSED_XZ) payload = decompressXzDataPayload(compressed);
    else payload = Buffer.from(compressed);
    return rowLifetime ? this.accessor.rowBytes(payload) : payload;
  }

  _readDataHeaderAt(offset) {
    const position = Number(offset);
    if (this._visibleSize() < position + DATA_OBJECT_HEADER_SIZE) {
      throw new Error('buffer too small for data object');
    }
    const oh = this._readObjectHeaderAt(position);
    if (!oh || oh.type !== OBJECT_TYPE_DATA || oh.size < BigInt(this.dataPayloadOffsetValue)) {
      throw new Error('corrupt DATA object');
    }
    return {
      hash: this._u64(position + 16),
      nextHashOffset: this._u64(position + 24),
      nextFieldOffset: this._u64(position + 32),
      entryOffset: this._u64(position + 40),
      entryArrayOffset: this._u64(position + 48),
      nEntries: this._u64(position + 56),
    };
  }

  _readFieldObjectAt(offset) {
    const position = Number(offset);
    if (this._visibleSize() < position + FIELD_OBJECT_HEADER_SIZE) {
      throw new Error('buffer too small for field object');
    }
    const oh = this._readObjectHeaderAt(position);
    if (!oh || oh.type !== OBJECT_TYPE_FIELD || oh.size < BigInt(FIELD_OBJECT_HEADER_SIZE)) {
      throw new Error('corrupt FIELD object');
    }
    const end = offset + oh.size;
    if (end > BigInt(this._visibleSize())) {
      throw new Error(`field object exceeds buffer at offset ${offset}`);
    }
    return {
      hash: this._u64(position + 16),
      nextHashOffset: this._u64(position + 24),
      headDataOffset: this._u64(position + 32),
      payload: this.accessor.tempView(position + FIELD_OBJECT_HEADER_SIZE, Number(end) - position - FIELD_OBJECT_HEADER_SIZE),
    };
  }

  _findFieldHeadDataOffset(field) {
    const tableOffset = this.header.field_hash_table_offset;
    const tableSize = this.header.field_hash_table_size;
    if (tableOffset === 0n || tableSize < BigInt(HASH_ITEM_SIZE)) return 0n;
    const h = this._hash(field);
    const buckets = tableSize / BigInt(HASH_ITEM_SIZE);
    if (buckets === 0n) return 0n;
    const bucketOffset = Number(tableOffset + (h % buckets) * BigInt(HASH_ITEM_SIZE));
    if (this._visibleSize() < bucketOffset + HASH_ITEM_SIZE) {
      throw new Error('field hash bucket exceeds buffer');
    }
    let offset = this._u64(bucketOffset);
    while (offset !== 0n) {
      const fieldObject = this._readFieldObjectAt(offset);
      if (fieldObject.hash === h && fieldObject.payload.equals(field)) {
        return fieldObject.headDataOffset;
      }
      offset = fieldObject.nextHashOffset;
    }
    return 0n;
  }

  _hash(payload) {
    if (this.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH) {
      return sipHash24(this.header.file_id, payload);
    }
    return jenkinsHash64(payload);
  }

  _readObjectHeaderAt(offset) {
    const position = Number(offset);
    if (this._visibleSize() < position + OBJECT_HEADER_SIZE) return null;
    const header = this.accessor.tempView(position, OBJECT_HEADER_SIZE);
    return parseObjectHeader(header, 0);
  }

  _dataObjectWasCompressedAt(offset) {
    const position = Number(offset);
    if (this._visibleSize() < position + 2) return false;
    return (this._u8(position + 1) & OBJECT_COMPRESSED_MASK) !== 0;
  }

  _u8(offset) {
    return this.accessor.u8(Number(offset));
  }

  _u32(offset) {
    return this.accessor.u32(Number(offset));
  }

  _u64(offset) {
    return this.accessor.u64(Number(offset));
  }

  _readBytes(offset, size) {
    return this.accessor.readBytes(Number(offset), Number(size));
  }

  _visibleSize() {
    return this.accessor.size();
  }
}

// Read and parse the file header without loading the entire file.
// For .zst whole-file-compressed files, decompresses the archive
// but reads only the header bytes from the decompressed output.
// Returns the parsed header object (same shape as FileReader.header).
export function readFileHeader(path) {
  let temp = null;
  let accessor = null;

  try {
    let openPath = path;
    let options = {};
    if (isZstFile(path)) {
      temp = streamZstToTempSync(path, { prefix: 'node-sdk-journal-header' });
      openPath = temp.path;
      options = withSnapshotBounds();
    }
    accessor = openReaderAccessor(openPath, options);
    return readHeaderFromAccessor(accessor);
  } finally {
    try { accessor?.close(); } catch {
      // Best-effort cleanup.
    }
    try { temp?.cleanup?.(); } catch {
      // Best-effort cleanup.
    }
  }
}

// Match filter (mirrors Go filterBuilder).
export class FilterBuilder {
  constructor() { this.level0 = []; this.level1 = []; this.current = []; }
  addMatch(data) {
    const item = Buffer.from(data);
    matchFieldName(item);
    this.current.push(item);
  }
  addDisjunction() { this._commitCurrent(); }
  addConjunction() { this._commitCurrent(); this._commitLevel1(); }

  _commitCurrent() {
    const expr = buildCurrentExpr(this.current);
    if (expr) this.level1.push(expr);
    this.current = [];
  }

  _commitLevel1() {
    const expr = buildOrExpr(this.level1);
    if (expr) this.level0.push(expr);
    this.level1 = [];
  }

  matches(entry) {
    const expr = this._finalExpr();
    if (!expr) return true;
    return expr.matches(entry);
  }

  _finalExpr() {
    const l0 = [...this.level0];
    const l1 = [...this.level1];
    const cur = buildCurrentExpr(this.current);
    if (cur) l1.push(cur);
    const l1Expr = buildOrExpr(l1);
    if (l1Expr) l0.push(l1Expr);
    if (l0.length === 0) return null;
    if (l0.length === 1) return l0[0];
    return new AndExpr(l0);
  }
}

class MatchExpr {
  constructor(field, value) { this.field = field; this.value = value; }
  matches(entry) {
    const vals = getOwn(entry.fieldValues, this.field);
    if (vals) return vals.some(v => Buffer.isBuffer(v) && v.equals(this.value));
    const v = getOwn(entry.fields, this.field);
    return v !== undefined && Buffer.isBuffer(v) && v.equals(this.value);
  }
}

class AndExpr {
  constructor(exprs) { this.exprs = exprs; }
  matches(entry) { return this.exprs.every(e => e.matches(entry)); }
}

class OrExpr {
  constructor(exprs) { this.exprs = exprs; }
  matches(entry) { return this.exprs.some(e => e.matches(entry)); }
}

const FALSE_EXPR = { matches: () => false };

function buildCurrentExpr(matches) {
  if (matches.length === 0) return null;
  const byField = new Map();
  const fieldOrder = [];
  for (const item of matches) {
    const eq = item.indexOf(0x3d);
    if (eq < 0) return FALSE_EXPR;
    const field = matchFieldName(item);
    let fieldMatches = byField.get(field);
    if (!fieldMatches) {
      fieldOrder.push(field);
      fieldMatches = [];
      byField.set(field, fieldMatches);
    }
    fieldMatches.push(new MatchExpr(field, item.slice(eq + 1)));
  }
  fieldOrder.sort();
  const parts = fieldOrder.map(f => {
    const vs = byField.get(f);
    return vs.length === 1 ? vs[0] : new OrExpr(vs);
  });
  return parts.length === 1 ? parts[0] : new AndExpr(parts);
}

function buildOrExpr(level1) {
  if (level1.length === 0) return null;
  if (level1.length === 1) return level1[0];
  return new OrExpr(level1);
}

function ensureSupportedHeader(header) {
  if (header.header_size < BigInt(HEADER_MIN_SIZE)) {
    throw new Error('unsupported journal: header size too small');
  }
  if (header.incompatible_flags & ~SUPPORTED_INCOMPATIBLE_FLAGS) {
    throw new Error('unsupported journal: incompatible flags ' + header.incompatible_flags.toString(16));
  }
}

function readHeaderFromAccessor(accessor) {
  if (accessor.size() < HEADER_MIN_SIZE) {
    throw new Error('file too small for journal header');
  }
  const headerSize = Math.min(HEADER_SIZE, accessor.size());
  return parseFileHeader(accessor.readBytes(0, headerSize));
}

function decodeUtf8OrNull(buf) {
  try {
    return utf8Decoder.decode(buf);
  } catch {
    return null;
  }
}

function fieldNameBytes(fieldName) {
  if (Buffer.isBuffer(fieldName)) return fieldName;
  if (fieldName instanceof Uint8Array) return Buffer.from(fieldName.buffer, fieldName.byteOffset, fieldName.byteLength);
  return Buffer.from(String(fieldName), 'utf8');
}

function fieldKey(fieldName) {
  return fieldNameBytes(fieldName).toString('hex');
}

function matchFieldName(item) {
  const eq = item.indexOf(0x3d);
  if (eq < 0) throw new Error('match must contain = separator');
  const field = decodeUtf8OrNull(item.slice(0, eq));
  if (field === null) throw new Error('match field name must be UTF-8');
  if (field === '') throw new Error('match field name must not be empty');
  return field;
}
