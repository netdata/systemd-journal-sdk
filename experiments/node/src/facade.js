// libsystemd-compatible reader facade for Node.js.

import { FileReader } from './lib/reader.js';
import { DirectoryReader } from './lib/directory-reader.js';
import { isJournalFileName } from './lib/compress.js';
import { parseMatchString } from './lib/hash.js';
import { uuidToString } from './lib/binary.js';
import { TextDecoder } from 'node:util';

const utf8Decoder = new TextDecoder('utf-8', { fatal: true });

// Format an entry as export bytes.
export function exportEntryBuffer(entry) {
  return buildExportEntryBuffer(entry);
}

function buildExportEntryBuffer(entry) {
  const parts = [];
  appendExportMetadata(parts, entry);
  appendPreferredExportFields(parts, entry);
  appendRemainingExportFields(parts, entry);
  appendRawExportFields(parts, entry);
  parts.push(Buffer.from('\n'));
  return Buffer.concat(parts);
}

function appendExportMetadata(parts, entry) {
  appendExportTextIfPresent(parts, '__CURSOR', entry.cursor);
  appendExportTextIfPresent(parts, '__REALTIME_TIMESTAMP', entry.realtime);
  appendExportTextIfPresent(parts, '__MONOTONIC_TIMESTAMP', entry.monotonic);
  appendExportTextIfPresent(parts, '__SEQNUM', entry.seqnum);
  appendExportTextIfPresent(parts, '__SEQNUM_ID', cursorSeqnumId(entry.cursor));
  appendExportTextIfPresent(parts, '_BOOT_ID', entry.boot_id && uuidToString(entry.boot_id));
}

function appendExportTextIfPresent(parts, name, value) {
  if (value) parts.push(formatExportText(`${name}=${value}`));
}

function appendPreferredExportFields(parts, entry) {
  const preferred = ['_MACHINE_ID', '_HOSTNAME', 'PRIORITY', '_TRANSPORT'];
  const written = new Set(['_BOOT_ID', '__CURSOR', '__REALTIME_TIMESTAMP', '__MONOTONIC_TIMESTAMP', '__SEQNUM', '__SEQNUM_ID']);
  for (const name of preferred) {
    const value = getOwnValue(entry.fields, name);
    if (value && !written.has(name)) {
      parts.push(formatExportField(name, value));
      written.add(name);
    }
  }
  return written;
}

function appendRemainingExportFields(parts, entry) {
  const written = new Set(['_BOOT_ID', '__CURSOR', '__REALTIME_TIMESTAMP', '__MONOTONIC_TIMESTAMP', '__SEQNUM', '__SEQNUM_ID']);
  for (const name of ['_MACHINE_ID', '_HOSTNAME', 'PRIORITY', '_TRANSPORT']) written.add(name);
  const remaining = Object.keys(entry.fields).filter(k => !written.has(k)).sort();
  for (const name of remaining) {
    appendExportFieldValues(parts, name, entry);
  }
}

function appendExportFieldValues(parts, name, entry) {
  const values = getOwnValue(entry.fieldValues, name);
  if (values) {
    for (const v of values) parts.push(formatExportField(name, v));
    return;
  }
  const value = getOwnValue(entry.fields, name);
  if (value) parts.push(formatExportField(name, value));
}

function appendRawExportFields(parts, entry) {
  for (const [name, value] of entry.rawFields || []) {
    if (decodeUtf8OrNull(name) === null) {
      parts.push(formatExportRawField(name, value));
    }
  }
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
  const result = Object.create(null);
  const written = addJsonMetadata(result, entry);
  addJsonFields(result, entry, written);
  return result;
}

function addJsonMetadata(result, entry) {
  const written = new Set();
  addJsonMetadataValue(result, written, '__CURSOR', entry.cursor);
  addJsonMetadataValue(result, written, '__REALTIME_TIMESTAMP', entry.realtime && String(entry.realtime));
  addJsonMetadataValue(result, written, '__MONOTONIC_TIMESTAMP', entry.monotonic && String(entry.monotonic));
  addJsonMetadataValue(result, written, '__SEQNUM', entry.seqnum && String(entry.seqnum));
  addJsonMetadataValue(result, written, '__SEQNUM_ID', cursorSeqnumId(entry.cursor));
  addJsonMetadataValue(result, written, '_BOOT_ID', entry.boot_id && uuidToString(entry.boot_id));
  return written;
}

function addJsonMetadataValue(result, written, name, value) {
  if (!value) return;
  setOwnValue(result, name, value);
  written.add(name);
}

function cursorSeqnumId(cursor) {
  if (!cursor) return '';
  for (const segment of String(cursor).split(';')) {
    if (segment.startsWith('s=')) return segment.slice(2);
  }
  return '';
}

function addJsonFields(result, entry, written) {
  const remaining = Object.keys(entry.fields).filter(k => !written.has(k)).sort();
  for (const name of remaining) {
    const fieldValues = getOwnValue(entry.fieldValues, name);
    const fieldValue = getOwnValue(entry.fields, name);
    const values = fieldValues || (fieldValue ? [fieldValue] : []);
    for (const v of values) addJsonValue(result, name, v);
  }
}

function addJsonValue(result, name, value) {
  const encoded = isPrintable(value, true) ? value.toString('utf8') : Array.from(value);
  const current = getOwnValue(result, name);
  if (current !== undefined) {
    if (Array.isArray(current)) current.push(encoded);
    else setOwnValue(result, name, [current, encoded]);
  } else {
    setOwnValue(result, name, encoded);
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

  static open(path, options = {}) {
    if (isJournalFileName(path.split('/').pop())) {
      return SdJournal.openFile(path, options);
    }
    return SdJournal.openDirectory(path, options);
  }

  static openFile(path, options = {}) {
    return new SdJournal(FileReader.open(path, options));
  }

  static openDirectory(path, options = {}) {
    return new SdJournal(DirectoryReader.open(path, options));
  }

  static openFiles(paths, options = {}) {
    if (paths.length === 1) return SdJournal.openFile(paths[0], options);
    return new SdJournal(DirectoryReader.openFiles(paths, options));
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
    const directPayload = getDirectEntryPayload(this.reader, fieldName);
    if (directPayload !== null) return directPayload;
    const entry = this.getEntry();
    return getEntryDataPayload(entry, fieldName);
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

  visitUniqueValues(fieldName, visitor) {
    const seen = new Set();
    for (const value of this.reader.queryUnique(fieldName)) {
      const key = Buffer.isBuffer(value) ? value.toString('binary') : String(value);
      if (seen.has(key)) continue;
      seen.add(key);
      visitor(Buffer.from(value));
    }
    return null;
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

export function SdJournalOpen(path, flags, options = {}) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open flags');
  return SdJournal.open(path, options);
}

export function SdJournalOpenFile(path, flags, options = {}) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_file flags');
  return SdJournal.openFile(path, options);
}

export function SdJournalOpenDirectory(path, flags, options = {}) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_directory flags');
  return SdJournal.openDirectory(path, options);
}

export function SdJournalOpenFiles(paths, flags, options = {}) {
  if (flags !== 0) throw new Error('unsupported sd_journal_open_files flags');
  return SdJournal.openFiles(paths, options);
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

export function SdJournalVisitUniqueValues(journal, fieldName, visitor) {
  journal.visitUniqueValues(fieldName, visitor);
  return null;
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

function getDirectEntryPayload(reader, fieldName) {
  if (typeof reader.getEntryPayload !== 'function') return null;
  const payload = reader.getEntryPayload(fieldName);
  return payload === null ? null : payload;
}

function getEntryDataPayload(entry, fieldName) {
  const nameBytes = fieldNameBytes(fieldName);
  const key = typeof fieldName === 'string' ? fieldName : decodeUtf8OrNull(nameBytes);
  const rawPayload = getRawEntryPayload(entry, key, nameBytes);
  if (rawPayload !== null) return rawPayload;
  const values = entry && key !== null ? getOwnValue(entry.fieldValues, key) : undefined;
  if (!entry || key === null || !values || values.length === 0) {
    throw new Error('data field not found');
  }
  return payloadFromFieldValue(key, values[0]);
}

function getRawEntryPayload(entry, key, nameBytes) {
  if (!entry || key !== null || !entry.rawFieldValues) return null;
  const rawValues = entry.rawFieldValues.get(nameBytes.toString('hex'));
  if (!rawValues || rawValues.length === 0) return null;
  return payloadFromFieldValue(nameBytes, rawValues[0]);
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
    for (const value of getOwnValue(entry.fieldValues, name) || []) {
      payloads.push(payloadFromFieldValue(name, value));
    }
  }
  return payloads;
}

function getOwnValue(object, key) {
  if (!object || !Object.hasOwn(object, key)) return undefined;
  // eslint-disable-next-line security/detect-object-injection -- journal field names are data; callers must be able to read arbitrary own fields from null-prototype maps.
  return object[key];
}

function setOwnValue(object, key, value) {
  // eslint-disable-next-line security/detect-object-injection -- journal field names are data; JSON/export output must preserve arbitrary own field names.
  object[key] = value;
}
