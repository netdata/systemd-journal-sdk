// Directory reader for iterating across multiple journal files.

import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { isJournalFileName } from './compress.js';
import { FileReader } from './reader.js';
import { uuidToString } from './binary.js';

export class DirectoryReader {
  constructor() {
    this.readers = [];
    this.index = 0;
    this.filter = null;
    this.realtimeSeek = null;
  }

  static open(path) {
    const dr = new DirectoryReader();

    for (const file of collectJournalFiles(path)) {
      try {
        dr.readers.push(FileReader.open(file));
      } catch { /* skip unreadable */ }
    }

    return DirectoryReader.fromReaders(dr.readers);
  }

  static openFiles(paths) {
    const readers = [];
    for (const path of paths) {
      if (!isJournalFileName(String(path).split('/').pop())) {
        throw new Error(`not a journal file: ${path}`);
      }
      readers.push(FileReader.open(path));
    }
    return DirectoryReader.fromReaders(readers);
  }

  static fromReaders(readers) {
    if (readers.length === 0) throw new Error('no journal files found');
    const dr = new DirectoryReader();
    dr.readers = readers;
    dr.readers.sort((a, b) => {
      const dt = a.header.head_entry_realtime - b.header.head_entry_realtime;
      if (dt !== 0n) return dt > 0n ? 1 : -1;
      const ds = a.header.head_entry_seqnum - b.header.head_entry_seqnum;
      return ds > 0n ? 1 : ds < 0n ? -1 : 0;
    });
    return dr;
  }

  close() {
    for (const r of this.readers) r.close();
    this.readers = [];
  }

  seekHead() {
    this.realtimeSeek = null;
    this.index = 0;
    if (this.readers.length > 0) this.readers[0].seekHead();
  }

  seekTail() {
    this.realtimeSeek = null;
    this.index = this.readers.length - 1;
    if (this.readers.length > 0) this.readers[this.index].seekTail();
  }

  seekRealtimeUsec(usec) {
    this.realtimeSeek = BigInt(usec);
  }

  step() {
    this._applyRealtimeSeek(0);
    while (this.index < this.readers.length) {
      const r = this.readers[this.index];
      if (r.step()) {
        if (!this.filter) return true;
        const entry = r.getEntry();
        if (entry && this.filter.matches(entry)) return true;
        continue;
      }
      this.index++;
      if (this.index < this.readers.length) this.readers[this.index].seekHead();
    }
    return false;
  }

  stepBack() {
    this._applyRealtimeSeek(1);
    while (this.index >= 0 && this.index < this.readers.length) {
      const r = this.readers[this.index];
      if (r.stepBack()) {
        if (!this.filter) return true;
        const entry = r.getEntry();
        if (entry && this.filter.matches(entry)) return true;
        continue;
      }
      this.index--;
      if (this.index >= 0) this.readers[this.index].seekTail();
    }
    return false;
  }

  _applyRealtimeSeek(direction) {
    if (this.realtimeSeek === null) return;
    const usec = this.realtimeSeek;
    this.realtimeSeek = null;
    if (this.readers.length === 0) {
      this.index = 0;
      return;
    }
    if (direction === 0) {
      let idx = this.readers.findIndex(r => r.header.tail_entry_realtime >= usec);
      if (idx < 0) idx = this.readers.length;
      this.index = idx;
      if (idx < this.readers.length) this.readers[idx].seekRealtimeUsec(usec);
      return;
    }

    let idx = -1;
    for (let i = this.readers.length - 1; i >= 0; i--) {
      if (this.readers[i].header.head_entry_realtime <= usec) {
        idx = i;
        break;
      }
    }
    if (idx < 0) {
      this.index = this.readers.length;
      return;
    }
    this.index = idx;
    this.readers[idx].seekRealtimeUsec(usec);
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

// Minimal filter builder for directory-level propagation.
// Each reader also gets matches pushed individually for per-file filtering.
import { FilterBuilder as _FB } from './reader.js';
const FilterBuilder = _FB;

function collectJournalFiles(path) {
  const files = [];
  const entries = readdirSync(path, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(path, entry.name);
    if (entry.isFile() && isJournalFileName(entry.name)) {
      files.push(fullPath);
    }
  }

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const childPath = join(path, entry.name);
    for (const child of readdirSync(childPath, { withFileTypes: true })) {
      if (child.isFile() && isJournalFileName(child.name)) {
        files.push(join(childPath, child.name));
      }
    }
  }

  return files;
}
