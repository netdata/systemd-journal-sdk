// Single journal file reader.
// Reads .journal, .journal~, .journal.zst, .journal~.zst files.
// Uses entry-array-based iteration (matching Go/Rust).

import { readFileSync, openSync, readSync, closeSync, statSync, existsSync, unlinkSync, rmdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { readUint64LE, uuidToString, align8 } from './binary.js';
import {
  parseFileHeader, parseObjectHeader,
  HEADER_MIN_SIZE, HEADER_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
  OBJECT_TYPE_DATA, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
  DATA_OBJECT_HEADER_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE, INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPACT,
  COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
  INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_LZ4,
} from './header.js';
import { decompressZstToTemp, isZstFile } from './compress.js';
import { parseEntryObject, parseDataObject } from './entry.js';

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

      if (header.header_size < BigInt(HEADER_MIN_SIZE)) {
        throw new Error('unsupported journal: header size too small');
      }

      // Must have keyed hash
      if (!(header.incompatible_flags & INCOMPATIBLE_KEYED_HASH)) {
        throw new Error('unsupported journal: keyed hash required');
      }

      const supported = INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_ZSTD | INCOMPATIBLE_COMPRESSED_LZ4 | INCOMPATIBLE_COMPACT;
      if (header.incompatible_flags & ~supported) {
        throw new Error('unsupported journal: incompatible flags ' + header.incompatible_flags.toString(16));
      }

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
    if (this.header.entry_array_offset === 0n) {
      this.entryOffsets = [];
      return;
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
      const itemSize = this._offsetArrayItemSize();
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

    this.entryOffsets = offsets;
    this.entryIndex = -1;
  }

  _validEntryOffset(offset) {
    const off = Number(offset);
    if (off + OBJECT_HEADER_SIZE > this.buffer.length) return false;
    const oh = parseObjectHeader(this.buffer, off);
    if (!oh) return false;
    if (oh.type === 0 && oh.size === 0n) return false;
    if (oh.type !== OBJECT_TYPE_ENTRY) return false;
    return true;
  }

  seekHead() { this.entryIndex = -1; this.direction = 0; this.realtimeSeek = null; }
  seekTail() { this.entryIndex = this.entryOffsets.length; this.direction = 1; this.realtimeSeek = null; }

  seekRealtimeUsec(usec) {
    this.realtimeSeek = BigInt(usec);
  }

  next() {
    if (this.realtimeSeek !== null) {
      const idx = this._firstRealtimeIndexAtOrAfter(this.realtimeSeek);
      this.realtimeSeek = null;
      this.direction = 0;
      if (idx >= this.entryOffsets.length) {
        this.entryIndex = this.entryOffsets.length;
        return false;
      }
      this.entryIndex = idx;
      return true;
    }
    this.direction = 0;
    this.entryIndex++;
    if (this.entryIndex >= this.entryOffsets.length) {
      this.entryIndex = this.entryOffsets.length;
      return false;
    }
    return true;
  }

  previous() {
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
      try {
        const entry = this.getEntry();
        if (entry && this.filter.matches(entry)) return true;
      } catch { /* skip corrupt */ }
    }
  }

  stepBack() {
    for (;;) {
      if (!this.previous()) return false;
      if (!this.filter) return true;
      try {
        const entry = this.getEntry();
        if (entry && this.filter.matches(entry)) return true;
      } catch { /* skip corrupt */ }
    }
  }

  getEntry() {
    if (this.entryIndex < 0 || this.entryIndex >= this.entryOffsets.length) return null;
    return this._readEntryAt(this.entryOffsets[this.entryIndex]);
  }

  _readEntryAt(offset) {
    const off = Number(offset);
    const e = parseEntryObject(this.buffer, off, this._isCompact());

    const fields = Object.create(null);
    const fieldValues = Object.create(null);
    const payloads = [];

    for (const item of e.items) {
      try {
        const { name, value } = parseDataObject(this.buffer, Number(item.offset), this._isCompact());
        const nameStr = name.toString('utf8');
        const valueBuf = Buffer.from(value);
        payloads.push(Buffer.concat([Buffer.from(name), Buffer.from('='), valueBuf]));
        if (!(nameStr in fields)) fields[nameStr] = valueBuf;
        if (!fieldValues[nameStr]) fieldValues[nameStr] = [];
        fieldValues[nameStr].push(valueBuf);
      } catch { /* skip corrupt data */ }
    }

    const cursor = this._makeCursor(offset, e);
    return {
      fields,
      fieldValues,
      payloads,
      seqnum: e.seqnum,
      realtime: e.realtime,
      monotonic: e.monotonic,
      boot_id: e.boot_id,
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
    const e = parseEntryObject(this.buffer, Number(offset), this._isCompact());
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
    const seen = new Set();
    const results = [];
    for (const off of this.entryOffsets) {
      try {
        const entry = this._readEntryAt(off);
        if (entry && entry.fieldValues[fieldName]) {
          for (const v of entry.fieldValues[fieldName]) {
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
    if (this.cleanupPath) {
      try { unlinkSync(this.cleanupPath); } catch {}
      try { rmdirSync(dirname(this.cleanupPath)); } catch {}
      this.cleanupPath = null;
    }
    this.buffer = null;
  }

  _isCompact() {
    return (this.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;
  }

  _offsetArrayItemSize() {
    return this._isCompact() ? COMPACT_OFFSET_ARRAY_ITEM_SIZE : REGULAR_OFFSET_ARRAY_ITEM_SIZE;
  }
}

// Match filter (mirrors Go filterBuilder).
export class FilterBuilder {
  constructor() { this.level0 = []; this.level1 = []; this.current = []; }
  addMatch(data) { this.current.push(Buffer.from(data)); }
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
    const field = item.slice(0, eq).toString('utf8');
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
