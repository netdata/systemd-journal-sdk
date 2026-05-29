// Directory reader for iterating across multiple journal files.

import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { isJournalFileName } from './compress.js';
import { FileReader, FilterBuilder } from './reader.js';
import { uuidToString } from './binary.js';

export class DirectoryReader {
  constructor() {
    this.readers = [];
    this.index = -1;
    this.filter = null;
    this.realtimeSeek = null;
    this.realtimeSeekBound = null;
    this.candidates = [];
    this.currentKey = null;
    this.direction = null;
    this.bootNewest = new Map();
  }

  static open(path) {
    const dr = new DirectoryReader();

    for (const file of collectJournalFiles(path)) {
      try {
        dr.readers.push(FileReader.open(file));
      } catch { /* skip unreadable */ }
    }

    return DirectoryReader.fromReaders(dr.readers, true);
  }

  static openFiles(paths) {
    const readers = [];
    for (const path of paths) {
      if (!isJournalFileName(String(path).split('/').pop())) {
        throw new Error(`not a journal file: ${path}`);
      }
      readers.push(FileReader.open(path));
    }
    return DirectoryReader.fromReaders(readers, false);
  }

  static fromReaders(readers, allowEmpty = false) {
    if (readers.length === 0 && !allowEmpty) throw new Error('no journal files found');
    const dr = new DirectoryReader();
    dr.readers = readers;
    dr.candidates = Array(readers.length).fill(null);
    dr.bootNewest = buildBootNewest(readers);
    dr.readers.sort((a, b) => {
      const dt = a.header.head_entry_realtime - b.header.head_entry_realtime;
      if (dt !== 0n) return dt > 0n ? 1 : -1;
      const ds = a.header.head_entry_seqnum - b.header.head_entry_seqnum;
      return ds > 0n ? 1 : ds < 0n ? -1 : 0;
    });
    return dr;
  }

  close() {
    let firstError = null;
    for (const r of this.readers) {
      try {
        r.close();
      } catch (error) {
        if (!firstError) firstError = error;
      }
    }
    this.readers = [];
    if (firstError) throw firstError;
  }

  seekHead() {
    this.realtimeSeek = null;
    this.realtimeSeekBound = null;
    this.index = -1;
    this.currentKey = null;
    this.direction = null;
    this._resetCandidates();
    for (const reader of this.readers) reader.seekHead();
  }

  seekTail() {
    this.realtimeSeek = null;
    this.realtimeSeekBound = null;
    this.index = -1;
    this.currentKey = null;
    this.direction = null;
    this._resetCandidates();
    for (const reader of this.readers) reader.seekTail();
  }

  seekRealtimeUsec(usec) {
    this.realtimeSeek = BigInt(usec);
    this.realtimeSeekBound = null;
    this.index = -1;
    this.currentKey = null;
    this.direction = null;
    this._resetCandidates();
  }

  step() {
    return this._stepMerged(0, true);
  }

  stepBack() {
    return this._stepMerged(1, true);
  }

  next() {
    return this.step();
  }

  previous() {
    return this.stepBack();
  }

  _stepMerged(direction, applyFilter) {
    this._prepareMergeDirection(direction);

    let best = null;
    for (let i = 0; i < this.readers.length; i++) {
      this._fillCandidate(i, direction, applyFilter);
      const candidate = this.candidates[i];
      if (!candidate) continue;
      if (!best) {
        best = candidate;
        continue;
      }
      const cmp = this._compareEntryKeys(candidate.key, best.key);
      if ((direction === 0 && cmp < 0) || (direction === 1 && cmp > 0)) {
        best = candidate;
      }
    }

    if (!best) {
      this.index = -1;
      this.realtimeSeekBound = null;
      return false;
    }

    this.index = best.readerIndex;
    this.currentKey = best.key;
    this.candidates[best.readerIndex] = null;
    this.realtimeSeekBound = null;
    return true;
  }

  _prepareMergeDirection(direction) {
    if (this.realtimeSeek !== null) {
      const usec = this.realtimeSeek;
      this.realtimeSeek = null;
      for (const reader of this.readers) reader.seekRealtimeUsec(usec);
      this._resetCandidates();
      this.realtimeSeekBound = { usec, direction };
      this.direction = direction;
      return;
    }

    if (this.direction === direction) return;

    if (this.currentKey) {
      for (const reader of this.readers) reader.seekRealtimeUsec(this.currentKey.realtime);
    } else if (direction === 0) {
      for (const reader of this.readers) reader.seekHead();
    } else {
      for (const reader of this.readers) reader.seekTail();
    }

    this._resetCandidates();
    this.direction = direction;
  }

  _fillCandidate(readerIndex, direction, applyFilter) {
    if (this.candidates[readerIndex]) return;
    const reader = this.readers[readerIndex];

    for (;;) {
      const ok = direction === 0 ? reader.next() : reader.previous();
      if (!ok) return;

      let key = null;
      try {
        key = reader.currentEntryKey();
      } catch {
        continue;
      }
      if (!key) continue;
      if (applyFilter && this.filter) {
        let entry = null;
        try {
          entry = reader.getEntry();
        } catch {
          continue;
        }
        if (!entry || !this.filter.matches(entry)) continue;
      }

      if (this.realtimeSeekBound) {
        const { usec, direction: seekDirection } = this.realtimeSeekBound;
        if ((seekDirection === 0 && key.realtime < usec) || (seekDirection === 1 && key.realtime > usec)) {
          continue;
        }
      }
      if (this.currentKey) {
        const cmp = this._compareEntryKeys(key, this.currentKey);
        if ((direction === 0 && cmp <= 0) || (direction === 1 && cmp >= 0)) continue;
      }

      this.candidates[readerIndex] = { readerIndex, key };
      return;
    }
  }

  _entryKey(reader, entry) {
    return {
      seqnumId: Buffer.from(reader.header.seqnum_id),
      seqnum: entry.seqnum,
      bootId: Buffer.from(entry.boot_id),
      monotonic: entry.monotonic,
      realtime: entry.realtime,
      xorHash: entry.xor_hash,
    };
  }

  _compareEntryKeys(a, b) {
    if (bufferEqual(a.bootId, b.bootId) &&
        a.monotonic === b.monotonic &&
        a.realtime === b.realtime &&
        a.xorHash === b.xorHash &&
        bufferEqual(a.seqnumId, b.seqnumId) &&
        a.seqnum === b.seqnum) {
      return 0;
    }
    if (bufferEqual(a.seqnumId, b.seqnumId)) {
      const cmp = cmpBigInt(a.seqnum, b.seqnum);
      if (cmp !== 0) return cmp;
    }
    if (bufferEqual(a.bootId, b.bootId)) {
      const cmp = cmpBigInt(a.monotonic, b.monotonic);
      if (cmp !== 0) return cmp;
    } else {
      const cmp = this._compareBootIds(a.bootId, b.bootId);
      if (cmp !== 0) return cmp;
    }
    const realtimeCmp = cmpBigInt(a.realtime, b.realtime);
    if (realtimeCmp !== 0) return realtimeCmp;
    return cmpBigInt(a.xorHash, b.xorHash);
  }

  _compareBootIds(a, b) {
    const aNewest = this.bootNewest.get(bufferKey(a));
    const bNewest = this.bootNewest.get(bufferKey(b));
    if (!aNewest || !bNewest || !bufferEqual(aNewest.machineId, bNewest.machineId)) return 0;
    return cmpBigInt(aNewest.realtime, bNewest.realtime);
  }

  _resetCandidates() {
    this.candidates = Array(this.readers.length).fill(null);
  }

  getEntry() {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].getEntry();
  }

  getCursor() {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].getCursor();
  }

  testCursor(cursor) { return this.getCursor() === cursor; }

  getRealtimeUsec() {
    if (this.index < 0 || this.index >= this.readers.length) return 0n;
    return this.readers[this.index].getRealtimeUsec();
  }

  currentEntryKey() {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].currentEntryKey();
  }

  visitEntryPayloads(visitor) {
    if (this.index < 0 || this.index >= this.readers.length) throw new Error('no entry at current position');
    return this.readers[this.index].visitEntryPayloads(visitor);
  }

  collectEntryPayloads() {
    if (this.index < 0 || this.index >= this.readers.length) throw new Error('no entry at current position');
    return this.readers[this.index].collectEntryPayloads();
  }

  getEntryPayload(fieldName) {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].getEntryPayload(fieldName);
  }

  getRaw(fieldName) {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].getRaw(fieldName);
  }

  getRawValues(fieldName) {
    if (this.index < 0 || this.index >= this.readers.length) return [];
    return this.readers[this.index].getRawValues(fieldName);
  }

  entryDataRestart() {
    if (this.index < 0 || this.index >= this.readers.length) throw new Error('no entry at current position');
    return this.readers[this.index].entryDataRestart();
  }

  enumerateEntryPayload() {
    if (this.index < 0 || this.index >= this.readers.length) return null;
    return this.readers[this.index].enumerateEntryPayload();
  }

  clearEntryDataState() {
    if (this.index < 0 || this.index >= this.readers.length) return;
    this.readers[this.index].clearEntryDataState();
  }

  addMatch(data) {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addMatch(data);
    this._resetMergeState();
  }

  addDisjunction() {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addDisjunction();
    this._resetMergeState();
  }

  addConjunction() {
    if (!this.filter) this.filter = new FilterBuilder();
    this.filter.addConjunction();
    this._resetMergeState();
  }

  flushMatches() {
    this.filter = null;
    this._resetMergeState();
  }

  _resetMergeState() {
    this.index = -1;
    this.currentKey = null;
    this.direction = null;
    this.realtimeSeekBound = null;
    this._resetCandidates();
  }

  queryUnique(fieldName) {
    const seen = new Set();
    const results = [];
    for (const r of this.readers) {
      for (const v of r.queryUnique(fieldName)) {
        const key = v.toString('base64');
        if (!seen.has(key)) { seen.add(key); results.push(v); }
      }
    }
    return results;
  }

  enumerateFields() {
    const fields = new Set();
    for (const r of this.readers) {
      for (const f of r.enumerateFields()) fields.add(f);
    }
    return fields;
  }

  listBoots() {
    const bootMap = new Map();

    for (const r of this.readers) {
      const bootId = uuidToString(r.header.tail_entry_boot_id);
      const firstSeq = r.header.head_entry_seqnum;
      const lastSeq = r.header.tail_entry_seqnum;
      const firstTime = r.header.head_entry_realtime;
      const lastTime = r.header.tail_entry_realtime;

      if (bootMap.has(bootId)) {
        const e = bootMap.get(bootId);
        if (firstSeq < e.firstSeq) e.firstSeq = firstSeq;
        if (firstTime < e.firstTime) e.firstTime = firstTime;
        if (lastSeq > e.lastSeq) e.lastSeq = lastSeq;
        if (lastTime > e.lastTime) e.lastTime = lastTime;
      } else {
        bootMap.set(bootId, { firstSeq, lastSeq, firstTime, lastTime });
      }
    }

    const boots = Array.from(bootMap.entries())
      .map(([bootId, t]) => ({ bootId, ...t }))
      .sort((a, b) => Number(a.firstTime - b.firstTime));

    const base = 1 - boots.length;
    return boots.map((b, i) => ({
      index: base + i,
      boot_id: b.bootId,
      first_entry: Number(b.firstTime),
      last_entry: Number(b.lastTime),
    }));
  }
}

function collectJournalFiles(path) {
  const files = [];
  const entries = readdirSync(path, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(path, entry.name);
    if (isRegularFile(fullPath) && isJournalFileName(entry.name)) {
      files.push(fullPath);
    }
  }

  for (const entry of entries) {
    if (!isJournalSubdirName(entry.name)) continue;
    const childPath = join(path, entry.name);
    if (!isDirectory(childPath)) continue;
    for (const child of readDirEntries(childPath)) {
      const childFile = join(childPath, child.name);
      if (isRegularFile(childFile) && isJournalFileName(child.name)) {
        files.push(childFile);
      }
    }
  }

  return files.sort();
}

function isRegularFile(path) {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function readDirEntries(path) {
  try {
    return readdirSync(path, { withFileTypes: true });
  } catch {
    return [];
  }
}

function isJournalSubdirName(name) {
  if (name.includes('.')) return false;
  return id128StringValid(name);
}

function id128StringValid(s) {
  if (s.length === 32) return /^[0-9a-fA-F]{32}$/.test(s);
  if (s.length === 36) return /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(s);
  return false;
}

function buildBootNewest(readers) {
  const newest = new Map();
  for (const reader of readers) {
    const bootId = Buffer.from(reader.header.tail_entry_boot_id);
    if (bootId.every(byte => byte === 0)) continue;
    const key = bufferKey(bootId);
    const current = newest.get(key);
    if (!current || reader.header.tail_entry_monotonic > current.monotonic) {
      newest.set(key, {
        machineId: Buffer.from(reader.header.machine_id),
        monotonic: reader.header.tail_entry_monotonic,
        realtime: reader.header.tail_entry_realtime,
      });
    }
  }
  return newest;
}

function cmpBigInt(a, b) {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

function bufferEqual(a, b) {
  return Buffer.compare(a, b) === 0;
}

function bufferKey(buf) {
  return Buffer.from(buf).toString('hex');
}
