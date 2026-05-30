// libsystemd-compatible reader facade for Node.js.

import { FileReader } from './lib/reader.js';
import { DirectoryReader } from './lib/directory-reader.js';
import { isJournalFileName } from './lib/compress.js';
import { parseMatchString } from './lib/hash.js';
import { readUint64LE, uuidToString } from './lib/binary.js';
import { OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE } from './lib/header.js';
import { TextDecoder } from 'node:util';

const utf8Decoder = new TextDecoder('utf-8', { fatal: true });

// Format an entry as export bytes.
export function exportEntryBuffer(entry) {
  const parts = [];
  if (entry.cursor) parts.push(formatExportText(`__CURSOR=${entry.cursor}`));
  if (entry.realtime) parts.push(formatExportText(`__REALTIME_TIMESTAMP=${entry.realtime}`));
  if (entry.monotonic) parts.push(formatExportText(`__MONOTONIC_TIMESTAMP=${entry.monotonic}`));
  if (entry.seqnum) parts.push(formatExportText(`__SEQNUM=${entry.seqnum}`));

  // Extract seqnum_id from cursor
  const m = entry.cursor && entry.cursor.match(/s=([^;]+)/);
  if (m) parts.push(formatExportText(`__SEQNUM_ID=${m[1]}`));
  if (entry.boot_id) parts.push(formatExportText(`_BOOT_ID=${uuidToString(entry.boot_id)}`));

  const preferred = ['_MACHINE_ID', '_HOSTNAME', 'PRIORITY', '_TRANSPORT'];
  const written = new Set(['_BOOT_ID', '__CURSOR', '__REALTIME_TIMESTAMP', '__MONOTONIC_TIMESTAMP', '__SEQNUM', '__SEQNUM_ID']);
  for (const name of preferred) {
    if (entry.fields[name] && !written.has(name)) {
      parts.push(formatExportField(name, entry.fields[name]));
      written.add(name);
    }
  }

  const remaining = Object.keys(entry.fields).filter(k => !written.has(k)).sort();
  for (const name of remaining) {
    if (entry.fieldValues[name]) {
      for (const v of entry.fieldValues[name]) parts.push(formatExportField(name, v));
    } else if (entry.fields[name]) {
      parts.push(formatExportField(name, entry.fields[name]));
    }
  }
  for (const [name, value] of entry.rawFields || []) {
    if (decodeUtf8OrNull(name) === null) {
      parts.push(formatExportRawField(name, value));
    }
  }

  parts.push(Buffer.from('\n'));
  return Buffer.concat(parts);
}

export function exportEntry(entry) {
  return exportEntryBuffer(entry);
}

function formatExportText(line) {
  return Buffer.from(line + '\n', 'utf8');
}

function formatExportField(name, value) {
  const text = Buffer.concat([Buffer.from(name + '='), value]);
  if (isPrintable(text, false)) return Buffer.concat([text, Buffer.from('\n')]);
  // Binary export format
  const sizeBuf = Buffer.alloc(8);
  sizeBuf.writeBigUInt64LE(BigInt(value.length), 0);
  return Buffer.concat([Buffer.from(name + '\n', 'utf8'), sizeBuf, value, Buffer.from('\n')]);
}

function formatExportRawField(name, value) {
  const nameBuf = Buffer.from(name);
  const valueBuf = Buffer.from(value);
  if (isPrintable(valueBuf, false)) {
    return Buffer.concat([nameBuf, Buffer.from('='), valueBuf, Buffer.from('\n')]);
  }
  const sizeBuf = Buffer.alloc(8);
  sizeBuf.writeBigUInt64LE(BigInt(valueBuf.length), 0);
  return Buffer.concat([nameBuf, Buffer.from('\n'), sizeBuf, valueBuf, Buffer.from('\n')]);
}

// Format an entry as JSON object.
export function jsonEntry(entry) {
  const result = {};
  const written = new Set();

  if (entry.cursor) { result['__CURSOR'] = entry.cursor; written.add('__CURSOR'); }
  if (entry.realtime) { result['__REALTIME_TIMESTAMP'] = String(entry.realtime); written.add('__REALTIME_TIMESTAMP'); }
  if (entry.monotonic) { result['__MONOTONIC_TIMESTAMP'] = String(entry.monotonic); written.add('__MONOTONIC_TIMESTAMP'); }
  if (entry.seqnum) { result['__SEQNUM'] = String(entry.seqnum); written.add('__SEQNUM'); }
  const m = entry.cursor && entry.cursor.match(/s=([^;]+)/);
  if (m) { result['__SEQNUM_ID'] = m[1]; written.add('__SEQNUM_ID'); }
  if (entry.boot_id) { result['_BOOT_ID'] = uuidToString(entry.boot_id); written.add('_BOOT_ID'); }

  const remaining = Object.keys(entry.fields).filter(k => !written.has(k)).sort();
  for (const name of remaining) {
    const values = entry.fieldValues[name] || (entry.fields[name] ? [entry.fields[name]] : []);
    for (const v of values) addJsonValue(result, name, v);
  }
  return result;
}

function addJsonValue(result, name, value) {
  const encoded = isPrintable(value, true) ? value.toString('utf8') : Array.from(value);
  if (result[name] !== undefined) {
    if (Array.isArray(result[name])) result[name].push(encoded);
    else result[name] = [result[name], encoded];
  } else {
    result[name] = encoded;
  }
}

// Format entry as default text output (just MESSAGE).
export function textEntry(entry) {
  const msg = entry.fields['MESSAGE'];
  return (msg ? msg.toString('utf8') : '') + '\n';
}

function isPrintable(buf, allowNewline) {
  let text;
  try {
    text = utf8Decoder.decode(buf);
  } catch {
    return false;
  }
  for (const ch of text) {
    const cp = ch.codePointAt(0);
    if (cp < 0x20 && cp !== 0x09 && !(allowNewline && cp === 0x0a)) return false;
    if (cp >= 0x7f && cp <= 0x9f) return false;
  }
  return true;
}

function decodeUtf8OrNull(buf) {
  try {
    return utf8Decoder.decode(buf);
  } catch {
    return null;
  }
}

// Parse a cursor string.
export function parseCursor(cursor) {
  const parts = {};
  for (const seg of cursor.split(';')) {
    const eq = seg.indexOf('=');
    if (eq <= 0) throw new Error('invalid cursor format');
    parts[seg.slice(0, eq)] = seg.slice(eq + 1);
  }
  if (!parts['s'] || !parts['j'] || !parts['c'] || !parts['n']) {
    throw new Error('invalid cursor format');
  }
  return {
    seqnumId: parts['s'],
    bootId: parts['j'],
    realtime: BigInt('0x' + parts['c']),
    seqnum: BigInt(parts['n']),
  };
}

// sd_journal facade
export class SdJournal {
  constructor(reader) {
    this.reader = reader;
    this.outputMode = 'default';
    this.dataItems = [];
    this.dataIndex = 0;
    this.dataFromReader = false;
    this.fieldItems = [];
    this.fieldIndex = 0;
    this.uniqueItems = [];
    this.uniqueIndex = 0;
  }

  static open(path) {
    if (isJournalFileName(path.split('/').pop())) {
      return SdJournal.openFile(path);
    }
    return SdJournal.openDirectory(path);
  }

  static openFile(path) {
    return new SdJournal(FileReader.open(path));
  }

  static openDirectory(path) {
    return new SdJournal(DirectoryReader.open(path));
  }

  static openFiles(paths) {
    if (paths.length === 1) return SdJournal.openFile(paths[0]);
    return new SdJournal(DirectoryReader.openFiles(paths));
  }

  close() { this.reader.close(); }

  resetIterators() {
    this.dataItems = [];
    this.dataIndex = 0;
    this.dataFromReader = false;
    this.fieldItems = [];
    this.fieldIndex = 0;
    this.uniqueItems = [];
    this.uniqueIndex = 0;
    if (this.reader && typeof this.reader.clearEntryDataState === 'function') {
      this.reader.clearEntryDataState();
    }
  }

  addMatch(data) {
    const matchStr = Buffer.isBuffer(data) ? data.toString('binary') : String(data);
    this.resetIterators();
    this.reader.addMatch(parseMatchString(matchStr));
  }

  addDisjunction() { this.resetIterators(); this.reader.addDisjunction(); }
  addConjunction() { this.resetIterators(); this.reader.addConjunction(); }
  flushMatches() { this.resetIterators(); this.reader.flushMatches(); }
  seekHead() { this.resetIterators(); this.reader.seekHead(); }
  seekTail() { this.resetIterators(); this.reader.seekTail(); }
  seekRealtimeUsec(usec) { this.resetIterators(); this.reader.seekRealtimeUsec(usec); }
  setOutputMode(mode) { this.outputMode = mode; }

  next() {
    this.resetIterators();
    if (this.reader.step()) return 1;
    return 0;
  }

  previous() {
    this.resetIterators();
    if (this.reader.stepBack()) return 1;
    return 0;
  }

  getEntry() { return this.reader.getEntry(); }
  getCursor() { return this.reader.getCursor(); }
  testCursor(cursor) { return this.reader.testCursor(cursor); }
  getRealtimeUsec() { return this.reader.getRealtimeUsec(); }
  getSeqnum() {
    const entry = this.getEntry();
    if (!entry) throw new Error('no entry at current position');
    return { seqnum: entry.seqnum, seqnum_id: parseCursor(entry.cursor).seqnumId };
  }
  getMonotonicUsec() {
    const entry = this.getEntry();
    if (!entry) throw new Error('no entry at current position');
    return { monotonic: entry.monotonic, boot_id: entry.boot_id };
  }

  seekCursor(cursor) {
    const want = parseCursor(cursor);
    this.seekRealtimeUsec(want.realtime);
    while (this.next() !== 0) {
      const entry = this.getEntry();
      const got = parseCursor(entry.cursor);
      if (got.realtime > want.realtime) {
        break;
      }
      if (got.seqnumId === want.seqnumId &&
          got.bootId === want.bootId &&
          got.realtime === want.realtime &&
          got.seqnum === want.seqnum) {
        return;
      }
    }
  }

  restartData() {
    if (typeof this.reader.entryDataRestart === 'function') {
      this.reader.entryDataRestart();
      this.dataItems = [];
      this.dataIndex = 0;
      this.dataFromReader = true;
      return;
    }
    const entry = this.getEntry();
    if (!entry) throw new Error('no entry at current position');
    this.dataItems = [...(entry.payloads || payloadsFromEntry(entry))];
    this.dataIndex = 0;
    this.dataFromReader = false;
  }

  enumerateAvailableData() {
    if (this.dataFromReader) {
      return this.reader.enumerateEntryPayload();
    }
    if (this.dataIndex >= this.dataItems.length) return null;
    return this.dataItems[this.dataIndex++];
  }

  getData(fieldName) {
    if (typeof this.reader.getEntryPayload === 'function') {
      const payload = this.reader.getEntryPayload(fieldName);
      if (payload !== null) return payload;
    }
    const entry = this.getEntry();
    const nameBytes = fieldNameBytes(fieldName);
    const key = typeof fieldName === 'string' ? fieldName : decodeUtf8OrNull(nameBytes);
    if (entry && key === null && entry.rawFieldValues) {
      const rawValues = entry.rawFieldValues.get(nameBytes.toString('hex'));
      if (rawValues && rawValues.length > 0) return payloadFromFieldValue(nameBytes, rawValues[0]);
    }
    if (!entry || key === null || !entry.fieldValues[key] || entry.fieldValues[key].length === 0) {
      throw new Error('data field not found');
    }
    return payloadFromFieldValue(key, entry.fieldValues[key][0]);
  }

  processOutput(entry) {
    switch (this.outputMode) {
      case 'export': return exportEntryBuffer(entry);
      case 'json': return JSON.stringify(jsonEntry(entry)) + '\n';
      default: return textEntry(entry);
    }
  }

  listBoots() {
    if (this.reader instanceof DirectoryReader) return this.reader.listBoots();
    return [];
  }

  enumerateFields() {
    const fields = this.reader.enumerateFields();
    return Array.from(fields).sort();
  }

  restartFields() {
    this.fieldItems = this.enumerateFields();
    this.fieldIndex = 0;
  }

  enumerateField() {
    if (this.fieldIndex >= this.fieldItems.length) return null;
    return this.fieldItems[this.fieldIndex++];
  }

  queryUnique(fieldName) {
    const values = this.reader.queryUnique(fieldName);
    return values.map(v => [fieldName, Buffer.from(v)]);
  }

  queryUniqueState(fieldName) {
    const values = this.reader.queryUnique(fieldName);
    this.uniqueItems = values.map(v => payloadFromFieldValue(fieldName, v));
    this.uniqueIndex = 0;
  }

  restartUnique() { this.uniqueIndex = 0; }

  enumerateAvailableUnique() {
    if (this.uniqueIndex >= this.uniqueItems.length) return null;
    return Buffer.from(this.uniqueItems[this.uniqueIndex++]);
  }
}

// Output mode constants (used by journalctl CLI).
export const OUTPUT_MODE_DEFAULT = 'default';
export const OUTPUT_MODE_JSON = 'json';
export const OUTPUT_MODE_EXPORT = 'export';

// Standalone C-style wrapper functions for libsystemd-compatible API.
// These delegate to SdJournal class methods.

export function SdJournalOpen(path, flags) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open flags');
  return SdJournal.open(path);
}

export function SdJournalOpenFile(path, flags) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_file flags');
  return SdJournal.openFile(path);
}

export function SdJournalOpenDirectory(path, flags) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_directory flags');
  return SdJournal.openDirectory(path);
}

export function SdJournalOpenFiles(paths, flags) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_files flags');
  return SdJournal.openFiles(paths);
}

export function SdJournalClose(journal) {
  journal.close();
}

export function SdJournalAddMatch(journal, data) {
  journal.addMatch(data);
}

export function SdJournalAddDisjunction(journal) {
  journal.addDisjunction();
}

export function SdJournalAddConjunction(journal) {
  journal.addConjunction();
}

export function SdJournalFlushMatches(journal) {
  journal.flushMatches();
}

export function SdJournalNext(journal) {
  return journal.next();
}

export function SdJournalNextSkip(journal, skip) {
  let advanced = 0;
  for (let i = 0; i < skip; i++) {
    if (journal.next() === 0) break;
    advanced++;
  }
  return advanced;
}

export function SdJournalPrevious(journal) {
  return journal.previous();
}

export function SdJournalPreviousSkip(journal, skip) {
  let advanced = 0;
  for (let i = 0; i < skip; i++) {
    if (journal.previous() === 0) break;
    advanced++;
  }
  return advanced;
}

export function SdJournalSeekHead(journal) {
  journal.seekHead();
}

export function SdJournalSeekTail(journal) {
  journal.seekTail();
}

export function SdJournalSeekRealtimeUsec(journal, usec) {
  journal.seekRealtimeUsec(usec);
}

export function SdJournalSeekCursor(journal, cursor) {
  journal.seekCursor(cursor);
}

export function SdJournalGetEntry(journal) {
  return journal.getEntry();
}

export function SdJournalGetData(journal, fieldName) {
  return journal.getData(fieldName);
}

export function SdJournalRestartData(journal) {
  journal.restartData();
}

export function SdJournalEnumerateAvailableData(journal) {
  return journal.enumerateAvailableData();
}

export function SdJournalGetRealtimeUsec(journal) {
  return journal.getRealtimeUsec();
}

export function SdJournalGetSeqnum(journal) {
  return journal.getSeqnum();
}

export function SdJournalGetMonotonicUsec(journal) {
  return journal.getMonotonicUsec();
}

export function SdJournalGetCursor(journal) {
  return journal.getCursor();
}

export function SdJournalTestCursor(journal, cursor) {
  return journal.testCursor(cursor);
}

export function SdJournalEnumerateFields(journal) {
  return journal.enumerateFields();
}

export function SdJournalRestartFields(journal) {
  journal.restartFields();
}

export function SdJournalEnumerateField(journal) {
  return journal.enumerateField();
}

export function SdJournalQueryUnique(journal, fieldName) {
  return journal.queryUnique(fieldName);
}

export function SdJournalQueryUniqueState(journal, fieldName) {
  journal.queryUniqueState(fieldName);
}

export function SdJournalRestartUnique(journal) {
  journal.restartUnique();
}

export function SdJournalEnumerateAvailableUnique(journal) {
  return journal.enumerateAvailableUnique();
}

export function SdJournalListBoots(journal) {
  return journal.listBoots();
}

export function SdJournalSetOutputMode(journal, mode) {
  journal.setOutputMode(mode);
}

export function SdJournalProcessOutput(journal, entry) {
  return journal.processOutput(entry);
}

function payloadFromFieldValue(fieldName, value) {
  return Buffer.concat([fieldNameBytes(fieldName), Buffer.from('='), Buffer.from(value)]);
}

function fieldNameBytes(fieldName) {
  if (Buffer.isBuffer(fieldName)) return fieldName;
  if (fieldName instanceof Uint8Array) return Buffer.from(fieldName.buffer, fieldName.byteOffset, fieldName.byteLength);
  return Buffer.from(String(fieldName), 'utf8');
}

function payloadsFromEntry(entry) {
  const payloads = [];
  for (const name of Object.keys(entry.fieldValues || {}).sort()) {
    for (const value of entry.fieldValues[name]) {
      payloads.push(payloadFromFieldValue(name, value));
    }
  }
  return payloads;
}
