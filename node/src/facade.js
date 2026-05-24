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

// Parse a cursor string.
export function parseCursor(cursor) {
  const parts = {};
  for (const seg of cursor.split(';')) {
    const eq = seg.indexOf('=');
    if (eq < 0) continue;
    parts[seg.slice(0, eq)] = seg.slice(eq + 1);
  }
  return {
    seqnumId: parts['s'] || '',
    bootId: parts['j'] || '',
    realtime: parts['c'] ? BigInt('0x' + parts['c']) : 0n,
    seqnum: parts['n'] ? BigInt(parts['n']) : 0n,
  };
}

// sd_journal facade
export class SdJournal {
  constructor(reader) {
    this.reader = reader;
    this.outputMode = 'default';
  }

  static open(path) {
    let reader;
    if (isJournalFileName(path.split('/').pop())) {
      reader = FileReader.open(path);
    } else {
      reader = DirectoryReader.open(path);
    }
    return new SdJournal(reader);
  }

  close() { this.reader.close(); }

  addMatch(data) {
    const matchStr = Buffer.isBuffer(data) ? data.toString('binary') : String(data);
    this.reader.addMatch(parseMatchString(matchStr));
  }

  addDisjunction() { this.reader.addDisjunction(); }
  addConjunction() { this.reader.addConjunction(); }
  flushMatches() { this.reader.flushMatches(); }
  seekHead() { this.reader.seekHead(); }
  seekTail() { this.reader.seekTail(); }
  setOutputMode(mode) { this.outputMode = mode; }

  next() {
    if (this.reader.step()) return 1;
    return 0;
  }

  previous() {
    if (this.reader.stepBack()) return 1;
    return 0;
  }

  getEntry() { return this.reader.getEntry(); }
  getCursor() { return this.reader.getCursor(); }
  testCursor(cursor) { return this.reader.testCursor(cursor); }
  getRealtimeUsec() { return this.reader.getRealtimeUsec(); }

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

  queryUnique(fieldName) {
    const values = this.reader.queryUnique(fieldName);
    return values.map(v => [fieldName, Buffer.from(v)]);
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

export function SdJournalOpenDirectory(path, flags) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_directory flags');
  return SdJournal.open(path);
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

export function SdJournalPrevious(journal) {
  return journal.previous();
}

export function SdJournalSeekHead(journal) {
  journal.seekHead();
}

export function SdJournalSeekTail(journal) {
  journal.seekTail();
}

export function SdJournalGetEntry(journal) {
  return journal.getEntry();
}

export function SdJournalGetRealtimeUsec(journal) {
  return journal.getRealtimeUsec();
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

export function SdJournalQueryUnique(journal, fieldName) {
  return journal.queryUnique(fieldName);
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
