// Journal file writer. Creates regular, non-compact, keyed-hash journal files.
// Compatible with stock journalctl readers during live append.
// No native locking (Node.js has no flock); uses file descriptor sync.

import { openSync, writeSync, readSync, closeSync, ftruncateSync, fsyncSync, renameSync } from 'node:fs';
import { dirname } from 'node:path';
import { readUint64LE, writeUint64LE, writeUint32LE, writeUint8, align8, randomUUID, isZeroUUID, bufEqual } from './binary.js';
import {
  serializeFileHeader, parseFileHeader, parseObjectHeader, writeObjectHeader,
  HEADER_SIZE, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_FIELD,
  STATE_ONLINE, STATE_OFFLINE, STATE_ARCHIVED,
  INCOMPATIBLE_KEYED_HASH,
  OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
  FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE,
  DEFAULT_DATA_HASH_BUCKETS, DEFAULT_FIELD_HASH_BUCKETS,
  INITIAL_ENTRY_ARRAY_CAP, INITIAL_DATA_ENTRY_ARRAY_CAP,
} from './header.js';
import { sipHash24, jenkinsHash64 } from './hash.js';

export class Writer {
  constructor(fd, path) {
    this.fd = fd;
    this.path = path;
    this.header = null;
    this.appendOffset = 0n;
    this.nextSeqnum = 1n;
    this.bootId = null;
    this.started = 0;
    this.closed = false;
  }

  // Create or truncate a journal file.
  static create(path, opts = {}) {
    const fd = openSync(path, 'w+');
    ftruncateSync(fd, 0);
    const w = new Writer(fd, path);
    w._initialize(opts);
    return w;
  }

  // Open an existing journal file for appending.
  static open(path) {
    const fd = openSync(path, 'r+');
    const headerBuf = Buffer.alloc(HEADER_SIZE);
    const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
    if (bytesRead < HEADER_SIZE) { closeSync(fd); throw new Error('cannot read journal header'); }

    const header = parseFileHeader(headerBuf);
    if ((header.incompatible_flags & ~INCOMPATIBLE_KEYED_HASH) !== 0) {
      closeSync(fd); throw new Error('unsupported journal: incompatible flags');
    }
    if ((header.incompatible_flags & INCOMPATIBLE_KEYED_HASH) === 0) {
      closeSync(fd); throw new Error('unsupported journal: keyed hash required');
    }
    if (header.data_hash_table_offset === 0n || header.field_hash_table_offset === 0n || header.tail_object_offset === 0n) {
      closeSync(fd); throw new Error('invalid journal: missing hash tables');
    }

    const tailSize = readObjectSizeFromFd(fd, header.tail_object_offset);
    const now = Date.now();
    const monotonicBase = header.tail_entry_monotonic > 0n
      ? Number(header.tail_entry_monotonic / 1000n)
      : 0;

    const w = new Writer(fd, path);
    w.header = header;
    w.appendOffset = align8(header.tail_object_offset + tailSize);
    w.nextSeqnum = header.tail_entry_seqnum + 1n;
    w.bootId = Buffer.from(header.tail_entry_boot_id);
    if (isZeroUUID(w.bootId)) w.bootId = Buffer.from(header.file_id);
    w.started = now - monotonicBase;

    w.header.state = STATE_ONLINE;
    w._writeHeader();
    return w;
  }

  _initialize(opts) {
    const dataBuckets = opts.dataHashTableBuckets || DEFAULT_DATA_HASH_BUCKETS;
    const fieldBuckets = opts.fieldHashTableBuckets || DEFAULT_FIELD_HASH_BUCKETS;

    const dataSize = BigInt(dataBuckets * HASH_ITEM_SIZE);
    const fieldSize = BigInt(fieldBuckets * HASH_ITEM_SIZE);
    const dataOffset = BigInt(HEADER_SIZE + OBJECT_HEADER_SIZE);
    const fieldObjOffset = dataOffset + dataSize;
    const fieldOffset = fieldObjOffset + BigInt(OBJECT_HEADER_SIZE);
    const appendOffset = fieldOffset + fieldSize;

    const fileId = opts.fileId || randomUUID();
    const machineId = opts.machineId || randomUUID();
    const bootId = opts.bootId || randomUUID();
    const seqnumId = opts.seqnumId || randomUUID();

    this.header = {
      signature: 'LPKSHHRH',
      compatible_flags: 0,
      incompatible_flags: INCOMPATIBLE_KEYED_HASH,
      state: STATE_ONLINE,
      file_id: fileId,
      machine_id: machineId,
      tail_entry_boot_id: bootId,
      seqnum_id: seqnumId,
      header_size: BigInt(HEADER_SIZE),
      arena_size: appendOffset - BigInt(HEADER_SIZE),
      data_hash_table_offset: dataOffset,
      data_hash_table_size: dataSize,
      field_hash_table_offset: fieldOffset,
      field_hash_table_size: fieldSize,
      tail_object_offset: fieldObjOffset,
      n_objects: 2n,
      n_entries: 0n,
      tail_entry_seqnum: 0n,
      head_entry_seqnum: 0n,
      entry_array_offset: 0n,
      head_entry_realtime: 0n,
      tail_entry_realtime: 0n,
      tail_entry_monotonic: 0n,
    };

    this.bootId = Buffer.from(bootId);
    this.appendOffset = appendOffset;
    this.nextSeqnum = opts.headSeqnum ? BigInt(opts.headSeqnum) : 1n;

    ftruncateSync(this.fd, Number(appendOffset));
    this._writeHeader();

    // Data hash table object header
    const dhtBuf = Buffer.alloc(OBJECT_HEADER_SIZE);
    writeObjectHeader(dhtBuf, 0, OBJECT_TYPE_DATA_HASH_TABLE, 0, BigInt(OBJECT_HEADER_SIZE) + dataSize);
    writeSync(this.fd, dhtBuf, 0, OBJECT_HEADER_SIZE, Number(dataOffset - BigInt(OBJECT_HEADER_SIZE)));

    // Field hash table object header
    const fhtBuf = Buffer.alloc(OBJECT_HEADER_SIZE);
    writeObjectHeader(fhtBuf, 0, OBJECT_TYPE_FIELD_HASH_TABLE, 0, BigInt(OBJECT_HEADER_SIZE) + fieldSize);
    writeSync(this.fd, fhtBuf, 0, OBJECT_HEADER_SIZE, Number(fieldObjOffset));
  }

  _writeHeader() {
    const buf = Buffer.alloc(HEADER_SIZE);
    serializeFileHeader(buf, this.header);
    writeSync(this.fd, buf, 0, HEADER_SIZE, 0);
  }

  _writeUint64At(offset, value) {
    const buf = Buffer.alloc(8);
    writeUint64LE(buf, 0, value);
    writeSync(this.fd, buf, 0, 8, Number(offset));
  }

  _writeUUIDAt(offset, uuid) {
    writeSync(this.fd, uuid, 0, 16, Number(offset));
  }

  // Append a journal entry.
  append(fields, opts = {}) {
    if (this.closed) throw new Error('writer closed');
    if (fields.length === 0) throw new Error('empty entry');

    const now = Date.now();
    const realtime = opts.realtimeUsec ? BigInt(opts.realtimeUsec) : BigInt(now * 1000);
    const monotonic = opts.monotonicUsec ? BigInt(opts.monotonicUsec) : BigInt((now - this.started) * 1000);
    const bootId = opts.bootId && !isZeroUUID(opts.bootId) ? Buffer.from(opts.bootId) : this.bootId;

    // Build payloads
    const payloads = [];
    for (const field of fields) {
      const name = field.name;
      const valueBuf = Buffer.isBuffer(field.value) ? field.value : Buffer.from(field.value);
      _validateFieldName(name);
      const payload = Buffer.alloc(name.length + 1 + valueBuf.length);
      Buffer.from(name, 'utf8').copy(payload, 0);
      payload[name.length] = 0x3d;
      valueBuf.copy(payload, name.length + 1);
      payloads.push(payload);
    }

    // Write data objects, compute items and xor hash
    const items = [];
    let xorHash = 0n;
    for (const payload of payloads) {
      const { offset, hash } = this._addData(payload);
      items.push({ offset, hash });
      xorHash ^= jenkinsHash64(payload);
    }

    // Sort by offset, dedupe
    items.sort((a, b) => (a.offset < b.offset ? -1 : a.offset > b.offset ? 1 : 0));
    const deduped = [items[0]];
    for (let i = 1; i < items.length; i++) {
      if (items[i].offset !== deduped[deduped.length - 1].offset) deduped.push(items[i]);
    }

    // Write entry object
    const entryOffset = this.appendOffset;
    const entrySize = BigInt(ENTRY_OBJECT_HEADER_SIZE + deduped.length * REGULAR_ENTRY_ITEM_SIZE);
    const alignedSize = align8(entrySize);
    const entryBuf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(entryBuf, 0, OBJECT_TYPE_ENTRY, 0, entrySize);
    writeUint64LE(entryBuf, 16, this.nextSeqnum);
    writeUint64LE(entryBuf, 24, realtime);
    writeUint64LE(entryBuf, 32, monotonic);
    bootId.copy(entryBuf, 40);
    writeUint64LE(entryBuf, 56, xorHash);
    for (let i = 0; i < deduped.length; i++) {
      const off = ENTRY_OBJECT_HEADER_SIZE + i * REGULAR_ENTRY_ITEM_SIZE;
      writeUint64LE(entryBuf, off, deduped[i].offset);
      writeUint64LE(entryBuf, off + 8, deduped[i].hash);
    }
    writeSync(this.fd, entryBuf, 0, entryBuf.length, Number(this.appendOffset));
    this._objectAdded(entryOffset, entrySize);

    // Publish object reachability before entry count
    this._publishObjectMetadata();

    // Append to entry array and link data
    this._appendToEntryArray(entryOffset);
    for (const item of deduped) this._linkDataToEntry(item.offset, entryOffset);

    // Commit entry metadata last (so live readers see complete rows)
    this._entryAdded(realtime, monotonic, bootId);
    this._publishEntryMetadata();

    return { realtime, seqnum: this.nextSeqnum - 1n };
  }

  // Append a string-valued map with sorted keys.
  appendMap(fieldsMap) {
    const keys = Object.keys(fieldsMap).sort();
    return this.append(keys.map(k => ({ name: k, value: fieldsMap[k] })));
  }

  _hash(payload) { return sipHash24(this.header.file_id, payload); }

  _addData(payload) {
    const hash = this._hash(payload);
    const existing = this._findData(hash, payload);
    if (existing !== null) return { offset: existing, hash };

    const offset = this.appendOffset;
    const size = BigInt(DATA_OBJECT_HEADER_SIZE + payload.length);
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, 0, size);
    writeUint64LE(buf, 16, hash);
    // next_hash_offset (24), next_field_offset (32), entry_offset (40), entry_array_offset (48), n_entries (56) = 0
    payload.copy(buf, DATA_OBJECT_HEADER_SIZE);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);

    // Insert into data hash table
    this._appendHashItem(this.header.data_hash_table_offset, this.header.data_hash_table_size, OBJECT_TYPE_DATA, hash, offset);

    // Link to field
    const eqPos = payload.indexOf(0x3d);
    if (eqPos > 0) {
      const fieldPayload = payload.slice(0, eqPos);
      const fieldOffset = this._addField(fieldPayload);
      const fieldHeadData = this._readFieldHeadDataOffset(fieldOffset);
      // Set data.next_field_offset to old field head
      this._writeUint64At(offset + 32n, fieldHeadData);
      // Set field.head_data_offset to new data offset
      this._writeUint64At(fieldOffset + 32n, offset);
    }

    return { offset, hash };
  }

  _addField(payload) {
    const hash = this._hash(payload);
    const existing = this._findField(hash, payload);
    if (existing !== null) return existing;

    const offset = this.appendOffset;
    const size = BigInt(FIELD_OBJECT_HEADER_SIZE + payload.length);
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_FIELD, 0, size);
    writeUint64LE(buf, 16, hash);
    // next_hash_offset (24) = 0, head_data_offset (32) = 0
    payload.copy(buf, FIELD_OBJECT_HEADER_SIZE);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);

    this._appendHashItem(this.header.field_hash_table_offset, this.header.field_hash_table_size, OBJECT_TYPE_FIELD, hash, offset);
    return offset;
  }

  _findData(hash, payload) {
    const nBuckets = this.header.data_hash_table_size / BigInt(HASH_ITEM_SIZE);
    const bucketOff = this.header.data_hash_table_offset + (hash % nBuckets) * BigInt(HASH_ITEM_SIZE);
    const item = this._readHashItem(bucketOff);

    let offset = item.head;
    while (offset !== 0n) {
      const stored = this._readDataPayload(offset);
      if (stored && bufEqual(stored, payload)) return offset;
      const nextHash = this._readUint64At(offset + 24n);
      offset = nextHash;
    }
    return null;
  }

  _findField(hash, payload) {
    const nBuckets = this.header.field_hash_table_size / BigInt(HASH_ITEM_SIZE);
    const bucketOff = this.header.field_hash_table_offset + (hash % nBuckets) * BigInt(HASH_ITEM_SIZE);
    const item = this._readHashItem(bucketOff);

    let offset = item.head;
    while (offset !== 0n) {
      const stored = this._readFieldPayload(offset);
      if (stored && bufEqual(stored, payload)) return offset;
      const nextHash = this._readUint64At(offset + 24n);
      offset = nextHash;
    }
    return null;
  }

  _readHashItem(offset) {
    const buf = Buffer.alloc(HASH_ITEM_SIZE);
    readSync(this.fd, buf, 0, HASH_ITEM_SIZE, Number(offset));
    return { head: readUint64LE(buf, 0), tail: readUint64LE(buf, 8) };
  }

  _writeHashItem(offset, item) {
    const buf = Buffer.alloc(HASH_ITEM_SIZE);
    writeUint64LE(buf, 0, item.head);
    writeUint64LE(buf, 8, item.tail);
    writeSync(this.fd, buf, 0, HASH_ITEM_SIZE, Number(offset));
  }

  _readDataPayload(offset) {
    const objSize = readObjectSizeFromFd(this.fd, offset);
    const payloadLen = Number(objSize) - DATA_OBJECT_HEADER_SIZE;
    if (payloadLen <= 0) return null;
    const buf = Buffer.alloc(payloadLen);
    readSync(this.fd, buf, 0, payloadLen, Number(offset) + DATA_OBJECT_HEADER_SIZE);
    return buf;
  }

  _readFieldPayload(offset) {
    const objSize = readObjectSizeFromFd(this.fd, offset);
    const payloadLen = Number(objSize) - FIELD_OBJECT_HEADER_SIZE;
    if (payloadLen <= 0) return null;
    const buf = Buffer.alloc(payloadLen);
    readSync(this.fd, buf, 0, payloadLen, Number(offset) + FIELD_OBJECT_HEADER_SIZE);
    return buf;
  }

  _readFieldHeadDataOffset(offset) {
    return this._readUint64At(offset + 32n);
  }

  _readUint64At(offset) {
    const buf = Buffer.alloc(8);
    readSync(this.fd, buf, 0, 8, Number(offset));
    return readUint64LE(buf, 0);
  }

  _appendHashItem(tableOffset, tableSize, expectedType, hash, objectOffset) {
    const nBuckets = tableSize / BigInt(HASH_ITEM_SIZE);
    const bucketOff = tableOffset + (hash % nBuckets) * BigInt(HASH_ITEM_SIZE);
    const item = this._readHashItem(bucketOff);

    if (item.head !== 0n) {
      const head = readObjectHeaderFromFd(this.fd, item.head);
      if (!head || head.type !== expectedType) throw new Error('invalid journal: hash bucket object type mismatch');
    }
    if (item.tail !== 0n) {
      // Link previous tail to new object
      this._writeUint64At(item.tail + 24n, objectOffset);
    } else {
      item.head = objectOffset;
    }
    item.tail = objectOffset;
    this._writeHashItem(bucketOff, item);
  }

  _objectAdded(offset, size) {
    this.header.tail_object_offset = offset;
    this.appendOffset = align8(offset + size);
    this.header.n_objects++;
    this.header.arena_size = this.appendOffset - BigInt(HEADER_SIZE);
  }

  _entryAdded(realtime, monotonic, bootId) {
    this.header.n_entries++;
    if (this.header.head_entry_seqnum === 0n) this.header.head_entry_seqnum = this.nextSeqnum;
    if (this.header.head_entry_realtime === 0n) this.header.head_entry_realtime = realtime;
    this.header.tail_entry_seqnum = this.nextSeqnum;
    this.header.tail_entry_realtime = realtime;
    this.header.tail_entry_monotonic = monotonic;
    this.header.tail_entry_boot_id = Buffer.from(bootId);
    this.nextSeqnum++;
  }

  _publishObjectMetadata() {
    this._writeUint64At(96n, this.header.arena_size);
    this._writeUint64At(136n, this.header.tail_object_offset);
    this._writeUint64At(144n, this.header.n_objects);
  }

  _publishEntryMetadata() {
    this._writeUUIDAt(56n, this.header.tail_entry_boot_id);
    this._writeUint64At(160n, this.header.tail_entry_seqnum);
    this._writeUint64At(168n, this.header.head_entry_seqnum);
    this._writeUint64At(176n, this.header.entry_array_offset);
    this._writeUint64At(184n, this.header.head_entry_realtime);
    this._writeUint64At(192n, this.header.tail_entry_realtime);
    this._writeUint64At(200n, this.header.tail_entry_monotonic);
    // n_entries last (makes entry visible to live readers)
    this._writeUint64At(152n, this.header.n_entries);
  }

  _appendToEntryArray(entryOffset) {
    if (this.header.entry_array_offset === 0n) {
      const arrayOff = this._allocateOffsetArray(BigInt(INITIAL_ENTRY_ARRAY_CAP));
      this.header.entry_array_offset = arrayOff;
      this._writeArrayItem(arrayOff, 0n, entryOffset);
      return;
    }

    let remaining = this.header.n_entries;
    let offset = this.header.entry_array_offset;

    for (;;) {
      const { capacity, nextOffset } = this._readOffsetArrayHeader(offset);
      if (remaining < BigInt(capacity)) {
        this._writeArrayItem(offset, remaining, entryOffset);
        return;
      }
      remaining -= BigInt(capacity);
      if (nextOffset === 0n) {
        const newOff = this._allocateOffsetArray(BigInt(capacity) * 2n);
        this._writeUint64At(offset + 16n, newOff);
        this._writeArrayItem(newOff, 0n, entryOffset);
        return;
      }
      offset = nextOffset;
    }
  }

  _readOffsetArrayHeader(offset) {
    const buf = Buffer.alloc(OFFSET_ARRAY_OBJECT_HEADER_SIZE);
    readSync(this.fd, buf, 0, OFFSET_ARRAY_OBJECT_HEADER_SIZE, Number(offset));
    const oh = parseObjectHeader(buf, 0);
    if (!oh || oh.type !== OBJECT_TYPE_ENTRY_ARRAY) {
      throw new Error('invalid entry array object');
    }
    const capacity = Number((oh.size - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) / 8n);
    const nextOffset = readUint64LE(buf, 16);
    return { capacity, nextOffset };
  }

  _allocateOffsetArray(capacity) {
    const offset = this.appendOffset;
    const size = BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE) + capacity * 8n;
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_ENTRY_ARRAY, 0, size);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);
    this._publishObjectMetadata();
    return offset;
  }

  _writeArrayItem(arrayOffset, index, entryOffset) {
    this._writeUint64At(arrayOffset + BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE) + index * 8n, entryOffset);
  }

  _linkDataToEntry(dataOffset, entryOffset) {
    const nEntries = this._readUint64At(dataOffset + 56n);

    if (nEntries === 0n) {
      this._writeUint64At(dataOffset + 40n, entryOffset);
      this._writeUint64At(dataOffset + 56n, 1n);
    } else if (nEntries === 1n) {
      const arrayOff = this._allocateOffsetArray(BigInt(INITIAL_DATA_ENTRY_ARRAY_CAP));
      this._writeArrayItem(arrayOff, 0n, entryOffset);
      this._writeUint64At(dataOffset + 48n, arrayOff);
      this._writeUint64At(dataOffset + 56n, 2n);
    } else {
      const entryArrayOff = this._readUint64At(dataOffset + 48n);
      if (entryArrayOff === 0n) throw new Error('invalid journal: missing data entry array');
      this._appendToDataEntryArray(entryArrayOff, nEntries - 1n, entryOffset);
      this._writeUint64At(dataOffset + 56n, nEntries + 1n);
    }
  }

  _appendToDataEntryArray(arrayOffset, currentCount, entryOffset) {
    let remaining = currentCount;
    let offset = arrayOffset;
    for (;;) {
      const { capacity, nextOffset } = this._readOffsetArrayHeader(offset);
      if (remaining < BigInt(capacity)) {
        this._writeArrayItem(offset, remaining, entryOffset);
        return;
      }
      remaining -= BigInt(capacity);
      if (nextOffset === 0n) {
        const newOff = this._allocateOffsetArray(BigInt(capacity) * 2n);
        this._writeUint64At(offset + 16n, newOff);
        this._writeArrayItem(newOff, 0n, entryOffset);
        return;
      }
      offset = nextOffset;
    }
  }

  sync() {
    if (this.closed) throw new Error('writer closed');
    this._writeHeader();
    fsyncSync(this.fd);
  }

  close() {
    if (this.closed) return;
    let closeError = null;
    try {
      this.header.state = STATE_OFFLINE;
      this._writeHeader();
      fsyncSync(this.fd);
    } catch (error) {
      closeError = error;
    }
    try {
      closeSync(this.fd);
    } catch (error) {
      if (!closeError) closeError = error;
    }
    this.closed = true;
    if (closeError) throw closeError;
  }

  archiveTo(path) {
    if (this.closed) throw new Error('writer closed');
    this.header.state = STATE_ARCHIVED;
    this._writeHeader();
    fsyncSync(this.fd);
    try {
      renameSync(this.path, path);
      this.path = path;
      let closeError = null;
      try {
        syncParentDirectory(path);
      } catch (error) {
        closeError = error;
      }
      try {
        closeSync(this.fd);
      } catch (error) {
        if (!closeError) closeError = error;
      }
      this.closed = true;
      if (closeError) throw closeError;
    } catch (error) {
      if (this.closed) throw error;
      this.header.state = STATE_ONLINE;
      this._writeHeader();
      fsyncSync(this.fd);
      throw error;
    }
  }

  currentSize() { return this.appendOffset; }
}

// Read object size (uint64 at offset+8) from an fd.
function readObjectSizeFromFd(fd, offset) {
  const buf = Buffer.alloc(8);
  readSync(fd, buf, 0, 8, Number(offset) + 8);
  return readUint64LE(buf, 0);
}

function readObjectHeaderFromFd(fd, offset) {
  const buf = Buffer.alloc(OBJECT_HEADER_SIZE);
  readSync(fd, buf, 0, OBJECT_HEADER_SIZE, Number(offset));
  return parseObjectHeader(buf, 0);
}

function syncParentDirectory(path) {
  const dirFd = openSync(dirname(path), 'r');
  try {
    fsyncSync(dirFd);
  } finally {
    closeSync(dirFd);
  }
}

function _validateFieldName(name) {
  if (!name || name.length === 0) throw new Error('invalid field name: empty');
  if (name.length > 64) throw new Error(`invalid field name: too long (${name.length})`);
  if (name[0] >= '0' && name[0] <= '9') throw new Error(`invalid field name: starts with digit: ${name}`);
  for (let i = 0; i < name.length; i++) {
    const c = name.charCodeAt(i);
    if (c !== 0x5f && !(c >= 0x41 && c <= 0x5a) && !(c >= 0x30 && c <= 0x39)) {
      throw new Error(`invalid field name: bad char at ${i}: ${name}`);
    }
  }
}
