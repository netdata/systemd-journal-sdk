// Single journal file reader.
// Reads .journal, .journal~, .journal.zst, .journal~.zst files.
// Uses entry-array-based iteration (matching Go/Rust).

import { readFileSync, openSync, readSync, closeSync, statSync, unlinkSync, rmdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { TextDecoder } from 'node:util';
import { readUint64LE, uuidToString } from './binary.js';
import {
  parseFileHeader, parseObjectHeader,
  HEADER_MIN_SIZE, HEADER_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
  OBJECT_TYPE_DATA, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
  DATA_OBJECT_HEADER_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE, INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPACT,
  COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
  INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_LZ4,
  COMPACT_ENTRY_ITEM_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE,
} from './header.js';
import { decompressZstToTemp, isZstFile } from './compress.js';
import { parseEntryObject, parseDataPayload } from './entry.js';

const utf8Decoder = new TextDecoder('utf-8', { fatal: true });
const SUPPORTED_INCOMPATIBLE_FLAGS = INCOMPATIBLE_KEYED_HASH |
  INCOMPATIBLE_COMPRESSED_XZ |
  INCOMPATIBLE_COMPRESSED_ZSTD |
  INCOMPATIBLE_COMPRESSED_LZ4 |
  INCOMPATIBLE_COMPACT;

export class FileReader {
  constructor(buffer, header, path, cleanupPath) {
    this.buffer = buffer;
    this.header = header;
    this.path = path;
    this.cleanupPath = cleanupPath;

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
  static open(path) {
    let buffer;
    let cleanupPath = null;

    try {
      if (isZstFile(path)) {
        cleanupPath = decompressZstToTemp(path, 'node-sdk-journal');
        buffer = readFileSync(cleanupPath);
      } else {
        buffer = readFileSync(path);
      }

      if (buffer.length < HEADER_MIN_SIZE) {
        throw new Error('file too small for journal header');
      }

      const header = parseFileHeader(buffer);

      ensureSupportedHeader(header);

      return new FileReader(buffer, header, path, cleanupPath);
    } catch (err) {
      if (cleanupPath) {
        try { unlinkSync(cleanupPath); } catch {}
        try { rmdirSync(dirname(cleanupPath)); } catch {}
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

    const offsets = [];
    let offset = this.header.entry_array_offset;
    let remaining = this.header.n_entries;

    while (offset !== 0n && remaining > 0n) {
      const oh = parseObjectHeader(this.buffer, Number(offset));
      if (!oh || oh.type !== OBJECT_TYPE_ENTRY_ARRAY) {
        break;
      }
      const objSize = oh.size;
      if (objSize < BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) {
        break;
      }
      const nextOffset = readUint64LE(this.buffer, Number(offset) + 16);
      const itemSize = this.offsetArrayItemSizeValue;
      if ((objSize - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) % BigInt(itemSize) !== 0n) {
        throw new Error('entry array item payload has invalid compact alignment');
      }
      const capacity = Number((objSize - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) / BigInt(itemSize));

      const toRead = Number(remaining < BigInt(capacity) ? remaining : BigInt(capacity));
      const dataStart = Number(offset) + OFFSET_ARRAY_OBJECT_HEADER_SIZE;

      for (let i = 0; i < toRead; i++) {
        const itemOffset = dataStart + i * itemSize;
        const entryOff = this._isCompact()
          ? BigInt(this.buffer.readUInt32LE(itemOffset))
          : readUint64LE(this.buffer, itemOffset);
        if (entryOff !== 0n && this._validEntryOffset(entryOff)) {
          offsets.push(entryOff);
        }
      }

      remaining -= BigInt(toRead);
      offset = nextOffset;
    }

    return offsets;
  }

  refresh() {
    return this._refreshEntryOffsets();
  }

  _refreshEntryOffsets() {
    if (this.cleanupPath) return false;

    let newHeader = null;
    let newSize = 0;
    try {
      const stat = statSync(this.path);
      if (!stat.isFile() || stat.size <= 0) return false;
      newSize = stat.size;
      newHeader = this._readCurrentHeader();
    } catch {
      return false;
    }

    const sameHeaderState =
      newSize === this.buffer.length &&
      newHeader.n_entries === this.header.n_entries &&
      newHeader.tail_entry_array_offset === this.header.tail_entry_array_offset &&
      newHeader.tail_entry_array_n_entries === this.header.tail_entry_array_n_entries;
    if (sameHeaderState) {
      this.header = newHeader;
      this.entryIndex = Math.min(this.entryIndex, this.entryOffsets.length);
      return false;
    }

    const oldBuffer = this.buffer;
    const oldHeader = this.header;
    const oldOffsets = this.entryOffsets;
    const oldIndex = this.entryIndex;
    const oldCompact = this.compact;
    const oldEntryItemSize = this.entryItemSize;
    const oldOffsetArrayItemSize = this.offsetArrayItemSizeValue;
    const oldDataPayloadOffset = this.dataPayloadOffsetValue;

    try {
      const buffer = readFileSync(this.path);
      if (buffer.length < HEADER_MIN_SIZE) throw new Error('file too small for journal header');
      const header = parseFileHeader(buffer);
      ensureSupportedHeader(header);
      this.buffer = buffer;
      this.header = header;
      this._updateLayoutCache();
      this.entryOffsets = this._readEntryArrayOffsets();
      this.entryIndex = Math.min(oldIndex, this.entryOffsets.length);
      this._resetCachedEntryDataState();
      return (
        this.entryOffsets.length !== oldOffsets.length ||
        (
          this.entryOffsets.length > 0 &&
          oldOffsets.length > 0 &&
          this.entryOffsets[this.entryOffsets.length - 1] !== oldOffsets[oldOffsets.length - 1]
        )
      );
    } catch {
      this.buffer = oldBuffer;
      this.header = oldHeader;
      this.entryOffsets = oldOffsets;
      this.entryIndex = Math.min(oldIndex, this.entryOffsets.length);
      this.compact = oldCompact;
      this.entryItemSize = oldEntryItemSize;
      this.offsetArrayItemSizeValue = oldOffsetArrayItemSize;
      this.dataPayloadOffsetValue = oldDataPayloadOffset;
      this._resetCachedEntryDataState();
      return false;
    }
  }

  _readCurrentHeader() {
    const fd = openSync(this.path, 'r');
    try {
      const headerBuf = Buffer.alloc(HEADER_SIZE);
      const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
      if (bytesRead < HEADER_MIN_SIZE) throw new Error('file too small for journal header');
      const header = parseFileHeader(headerBuf.subarray(0, bytesRead));
      ensureSupportedHeader(header);
      return header;
    } finally {
      closeSync(fd);
    }
  }

  _validEntryOffset(offset) {
    const off = Number(offset);
    if (off + OBJECT_HEADER_SIZE > this.buffer.length) return false;
    const oh = parseObjectHeader(this.buffer, off);
    if (!oh) return false;
    if (oh.type === 0 && oh.size === 0n) return false;
    if (oh.type !== OBJECT_TYPE_ENTRY) return false;
    if (BigInt(off) + oh.size > BigInt(this.buffer.length)) return false;
    return true;
  }

  seekHead() {
    this._resetCachedEntryDataState();
    this.entryIndex = -1;
    this.direction = 0;
    this.realtimeSeek = null;
  }

  seekTail() {
    this._resetCachedEntryDataState();
    this.entryIndex = this.entryOffsets.length;
    this.direction = 1;
    this.realtimeSeek = null;
  }

  seekRealtimeUsec(usec) {
    this._resetCachedEntryDataState();
    this.realtimeSeek = BigInt(usec);
  }

  next() {
    this._resetCachedEntryDataState();
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
    this._resetCachedEntryDataState();
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
    const offset = this.entryOffsets[index];
    return readUint64LE(this.buffer, Number(offset) + OBJECT_HEADER_SIZE + 8);
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
        if (!(nameStr in fields)) fields[nameStr] = valueBuf;
        if (!fieldValues[nameStr]) fieldValues[nameStr] = [];
        fieldValues[nameStr].push(valueBuf);
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
    return readUint64LE(this.buffer, Number(offset) + OBJECT_HEADER_SIZE + 8);
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
    const rawKey = fieldKey(fieldNameBytes(fieldName));
    const stringKey = fieldNameStringOrNull(fieldName);
    const seen = new Set();
    const results = [];
    for (const off of this.entryOffsets) {
      try {
        const entry = this._readEntryAt(off);
        if (!entry) continue;
        const values = entry.rawFieldValues.get(rawKey) || (stringKey !== null ? entry.fieldValues[stringKey] : null);
        if (values) {
          for (const v of values) {
            const key = v.toString('base64');
            if (!seen.has(key)) { seen.add(key); results.push(v); }
          }
        }
      } catch {}
    }
    return results;
  }

  enumerateFields() {
    const fields = new Set();
    for (const off of this.entryOffsets) {
      try {
        const entry = this._readEntryAt(off);
        if (entry) for (const k of Object.keys(entry.fields)) fields.add(k);
      } catch {}
    }
    return fields;
  }

  close() {
    this._resetCachedEntryDataState();
    if (this.cleanupPath) {
      try { unlinkSync(this.cleanupPath); } catch {}
      try { rmdirSync(dirname(this.cleanupPath)); } catch {}
      this.cleanupPath = null;
    }
    this.buffer = null;
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
    const e = parseEntryObject(this.buffer, Number(offset), this.compact);
    return {
      entry: e,
      dataOffsets: includeOffsets ? e.items.map((item) => item.offset).filter((itemOffset) => itemOffset !== 0n) : [],
    };
  }

  _readDataPayloadAt(offset) {
    return parseDataPayload(this.buffer, Number(offset), this.compact);
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
    const vals = entry.fieldValues[this.field];
    if (vals) return vals.some(v => Buffer.isBuffer(v) && v.equals(this.value));
    const v = entry.fields[this.field];
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
  const byField = Object.create(null);
  const fieldOrder = [];
  for (const item of matches) {
    const eq = item.indexOf(0x3d);
    if (eq < 0) return FALSE_EXPR;
    const field = matchFieldName(item);
    if (!byField[field]) { fieldOrder.push(field); byField[field] = []; }
    byField[field].push(new MatchExpr(field, item.slice(eq + 1)));
  }
  fieldOrder.sort();
  const parts = fieldOrder.map(f => {
    const vs = byField[f];
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
  if (!(header.incompatible_flags & INCOMPATIBLE_KEYED_HASH)) {
    throw new Error('unsupported journal: keyed hash required');
  }
  if (header.incompatible_flags & ~SUPPORTED_INCOMPATIBLE_FLAGS) {
    throw new Error('unsupported journal: incompatible flags ' + header.incompatible_flags.toString(16));
  }
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

function fieldNameStringOrNull(fieldName) {
  if (typeof fieldName === 'string') return fieldName;
  return decodeUtf8OrNull(fieldNameBytes(fieldName));
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
