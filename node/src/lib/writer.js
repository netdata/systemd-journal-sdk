// Journal file writer. Creates regular, non-compact, keyed-hash journal files.
// Compatible with stock journalctl readers during live append.

import { openSync, writeSync, readSync, closeSync, ftruncateSync, fsyncSync, renameSync } from 'node:fs';
import { dirname } from 'node:path';
import { zstdCompressSync, zstdDecompressSync } from 'node:zlib';
import { readUint64LE, writeUint64LE, writeUint32LE, writeUint8, align8, randomUUID, isZeroUUID, bufEqual } from './binary.js';
import { WriterLock } from './lock.js';
import {
  serializeFileHeader, parseFileHeader, parseObjectHeader, writeObjectHeader,
  HEADER_SIZE, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_FIELD,
  STATE_OFFLINE, STATE_ONLINE, STATE_ARCHIVED,
  INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_LZ4,
  COMPATIBLE_TAIL_ENTRY_BOOT_ID,
  OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
  FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
  DEFAULT_DATA_HASH_BUCKETS, DEFAULT_FIELD_HASH_BUCKETS,
  FILE_SIZE_INCREASE,
  INITIAL_ENTRY_ARRAY_CAP, INITIAL_DATA_ENTRY_ARRAY_CAP,
} from './header.js';
import { sipHash24, jenkinsHash64 } from './hash.js';
import { compressLz4DataPayload, decompressLz4DataPayload } from './lz4-block.js';

export const COMPRESSION_NONE = 0;
export const COMPRESSION_ZSTD = 1;
export const COMPRESSION_LZ4 = 3;
export const DEFAULT_COMPRESS_THRESHOLD = 64;

export class Writer {
  constructor(fd, path, lock) {
    this.fd = fd;
    this.path = path;
    this.lock = lock;
    this.header = null;
    this.appendOffset = 0n;
    this.nextSeqnum = 1n;
    this.bootId = null;
    this.started = 0;
    this.closed = false;
    this.compression = COMPRESSION_NONE;
    this.compressThreshold = DEFAULT_COMPRESS_THRESHOLD;
  }

  // Create or truncate a journal file.
  static create(path, opts = {}) {
    const lock = WriterLock.acquire(path);
    let fd;
    try {
      fd = openSync(path, 'w+', 0o640);
      ftruncateSync(fd, 0);
      const w = new Writer(fd, path, lock);
      w.compression = normalizeCompression(opts.compression);
      w.compressThreshold = opts.compressionThresholdBytes ?? DEFAULT_COMPRESS_THRESHOLD;
      w._initialize(opts);
      return w;
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      lock.release();
      throw error;
    }
  }

  // Open an existing journal file for appending.
  static open(path) {
    const lock = WriterLock.acquire(path);
    let fd;
    try {
      fd = openSync(path, 'r+');
      const headerBuf = Buffer.alloc(HEADER_SIZE);
      const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
      if (bytesRead < HEADER_SIZE) throw new Error('cannot read journal header');

      const header = parseFileHeader(headerBuf);
      const supportedWriterIncompatible = INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_ZSTD | INCOMPATIBLE_COMPRESSED_LZ4;
      if ((header.incompatible_flags & ~supportedWriterIncompatible) !== 0) {
        throw new Error('unsupported journal: incompatible flags');
      }
      if ((header.incompatible_flags & INCOMPATIBLE_KEYED_HASH) === 0) {
        throw new Error('unsupported journal: keyed hash required');
      }
      if (header.data_hash_table_offset === 0n || header.field_hash_table_offset === 0n || header.tail_object_offset === 0n) {
        throw new Error('invalid journal: missing hash tables');
      }

      const tailSize = readObjectSizeFromFd(fd, header.tail_object_offset);
      const now = Date.now();
      const monotonicBase = header.tail_entry_monotonic > 0n
        ? Number(header.tail_entry_monotonic / 1000n)
        : 0;

      const w = new Writer(fd, path, lock);
      w.header = header;
      w.appendOffset = align8(header.tail_object_offset + tailSize);
      w.nextSeqnum = header.tail_entry_seqnum + 1n;
      w.bootId = Buffer.from(header.tail_entry_boot_id);
      if (isZeroUUID(w.bootId)) w.bootId = Buffer.from(header.file_id);
      w.started = now - monotonicBase;
      w.compression = (header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4) !== 0
        ? COMPRESSION_LZ4
        : ((header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD) !== 0 ? COMPRESSION_ZSTD : COMPRESSION_NONE);
      w.compressThreshold = DEFAULT_COMPRESS_THRESHOLD;

      w.header.state = STATE_ONLINE;
      w._writeHeader();
      return w;
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      lock.release();
      throw error;
    }
  }

  _initialize(opts) {
    const dataBuckets = opts.dataHashTableBuckets || DEFAULT_DATA_HASH_BUCKETS;
    const fieldBuckets = opts.fieldHashTableBuckets || DEFAULT_FIELD_HASH_BUCKETS;

    const dataSize = BigInt(dataBuckets * HASH_ITEM_SIZE);
    const fieldSize = BigInt(fieldBuckets * HASH_ITEM_SIZE);
    // systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE
    const fieldObjOffset = BigInt(HEADER_SIZE);
    const fieldOffset = fieldObjOffset + BigInt(OBJECT_HEADER_SIZE);
    const dataObjOffset = align8(fieldOffset + fieldSize);
    const dataOffset = dataObjOffset + BigInt(OBJECT_HEADER_SIZE);
    const appendOffset = align8(dataOffset + dataSize);
    const fileSize = BigInt(FILE_SIZE_INCREASE);

    const fileId = opts.fileId || randomUUID();
    const machineId = opts.machineId || randomUUID();
    const bootId = opts.bootId || randomUUID();
    const seqnumId = opts.seqnumId || randomUUID();

    let incFlags = INCOMPATIBLE_KEYED_HASH;
    if (this.compression === COMPRESSION_ZSTD) {
      incFlags |= INCOMPATIBLE_COMPRESSED_ZSTD;
    } else if (this.compression === COMPRESSION_LZ4) {
      incFlags |= INCOMPATIBLE_COMPRESSED_LZ4;
    }

    this.header = {
      signature: 'LPKSHHRH',
      compatible_flags: COMPATIBLE_TAIL_ENTRY_BOOT_ID,  // v260+ sets TAIL_ENTRY_BOOT_ID
      incompatible_flags: incFlags,
      state: STATE_ONLINE,
      file_id: fileId,
      machine_id: machineId,
      tail_entry_boot_id: Buffer.alloc(16),
      seqnum_id: seqnumId,
      header_size: BigInt(HEADER_SIZE),
      arena_size: fileSize - BigInt(HEADER_SIZE),
      data_hash_table_offset: dataOffset,
      data_hash_table_size: dataSize,
      field_hash_table_offset: fieldOffset,
      field_hash_table_size: fieldSize,
      tail_object_offset: dataObjOffset,
      n_objects: 2n,
      n_entries: 0n,
      tail_entry_seqnum: 0n,
      head_entry_seqnum: 0n,
      entry_array_offset: 0n,
      head_entry_realtime: 0n,
      tail_entry_realtime: 0n,
      tail_entry_monotonic: 0n,
      n_data: 0n,
      n_fields: 0n,
      n_tags: 0n,
      n_entry_arrays: 0n,
      data_hash_chain_depth: 0n,
      field_hash_chain_depth: 0n,
      tail_entry_array_offset: 0,
      tail_entry_array_n_entries: 0,
      tail_entry_offset: 0n,
    };

    this.bootId = Buffer.from(bootId);
    this.appendOffset = appendOffset;
    this.nextSeqnum = opts.headSeqnum ? BigInt(opts.headSeqnum) : 1n;

    ftruncateSync(this.fd, Number(fileSize));
    this._writeHeader();

    // systemd writes FIELD hash table first, then DATA hash table
    const fhtBuf = Buffer.alloc(OBJECT_HEADER_SIZE);
    writeObjectHeader(fhtBuf, 0, OBJECT_TYPE_FIELD_HASH_TABLE, 0, BigInt(OBJECT_HEADER_SIZE) + fieldSize);
    writeSync(this.fd, fhtBuf, 0, OBJECT_HEADER_SIZE, Number(fieldObjOffset));

    // Data hash table object header
    const dhtBuf = Buffer.alloc(OBJECT_HEADER_SIZE);
    writeObjectHeader(dhtBuf, 0, OBJECT_TYPE_DATA_HASH_TABLE, 0, BigInt(OBJECT_HEADER_SIZE) + dataSize);
    writeSync(this.fd, dhtBuf, 0, OBJECT_HEADER_SIZE, Number(dataObjOffset));
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

  _writeUint32At(offset, value) {
    const buf = Buffer.alloc(4);
    writeUint32LE(buf, 0, value);
    writeSync(this.fd, buf, 0, 4, Number(offset));
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
    this._entryAdded(entryOffset, realtime, monotonic, bootId);
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

    let objectPayload = payload;
    let compressionFlag = 0;
    if (this.compression === COMPRESSION_ZSTD && payload.length >= this.compressThreshold) {
      try {
        const compressed = zstdCompressSync(payload);
        if (compressed.length < payload.length) {
          objectPayload = compressed;
          compressionFlag = OBJECT_COMPRESSED_ZSTD;
        }
      } catch (_) {
        // compression failed, use uncompressed
      }
    } else if (this.compression === COMPRESSION_LZ4 && payload.length >= this.compressThreshold && payload.length >= 9) {
      try {
        const compressed = compressLz4DataPayload(payload);
        if (compressed && compressed.length < payload.length) {
          objectPayload = compressed;
          compressionFlag = OBJECT_COMPRESSED_LZ4;
        }
      } catch (_) {
        // compression failed, use uncompressed
      }
    }

    const size = BigInt(DATA_OBJECT_HEADER_SIZE + objectPayload.length);
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, compressionFlag, size);
    writeUint64LE(buf, 16, hash);
    objectPayload.copy(buf, DATA_OBJECT_HEADER_SIZE);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);

    // Insert into data hash table
    this._appendHashItem(this.header.data_hash_table_offset, this.header.data_hash_table_size, OBJECT_TYPE_DATA, hash, offset);
    this.header.n_data++;

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
    this.header.n_fields++;
    return offset;
  }

  _findData(hash, payload) {
    const nBuckets = this.header.data_hash_table_size / BigInt(HASH_ITEM_SIZE);
    const bucketOff = this.header.data_hash_table_offset + (hash % nBuckets) * BigInt(HASH_ITEM_SIZE);
    const item = this._readHashItem(bucketOff);

    let depth = 0n;
    let offset = item.head;
    while (offset !== 0n) {
      const stored = this._readDataPayload(offset);
      if (stored && bufEqual(stored, payload)) return offset;
      const nextHash = this._readUint64At(offset + 24n);
      if (nextHash !== 0n) {
        depth++;
        if (depth > this.header.data_hash_chain_depth) this.header.data_hash_chain_depth = depth;
      }
      offset = nextHash;
    }
    return null;
  }

  _findField(hash, payload) {
    const nBuckets = this.header.field_hash_table_size / BigInt(HASH_ITEM_SIZE);
    const bucketOff = this.header.field_hash_table_offset + (hash % nBuckets) * BigInt(HASH_ITEM_SIZE);
    const item = this._readHashItem(bucketOff);

    let depth = 0n;
    let offset = item.head;
    while (offset !== 0n) {
      const stored = this._readFieldPayload(offset);
      if (stored && bufEqual(stored, payload)) return offset;
      const nextHash = this._readUint64At(offset + 24n);
      if (nextHash !== 0n) {
        depth++;
        if (depth > this.header.field_hash_chain_depth) this.header.field_hash_chain_depth = depth;
      }
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
    const objHeader = readObjectHeaderFromFd(this.fd, offset);
    if (!objHeader || objHeader.type !== OBJECT_TYPE_DATA) return null;
    const objSize = objHeader.size;
    const payloadLen = Number(objSize) - DATA_OBJECT_HEADER_SIZE;
    if (payloadLen <= 0) return null;
    const buf = Buffer.alloc(payloadLen);
    readSync(this.fd, buf, 0, payloadLen, Number(offset) + DATA_OBJECT_HEADER_SIZE);
    if ((objHeader.flags & OBJECT_COMPRESSED_ZSTD) !== 0) {
      return zstdDecompressSync(buf);
    }
    if ((objHeader.flags & OBJECT_COMPRESSED_LZ4) !== 0) {
      return decompressLz4DataPayload(buf);
    }
    if ((objHeader.flags & OBJECT_COMPRESSED_XZ) !== 0) {
      throw new Error(`unsupported DATA object compression flags: 0x${objHeader.flags.toString(16)}`);
    }
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
  }

  _entryAdded(entryOffset, realtime, monotonic, bootId) {
    this.header.n_entries++;
    if (this.header.head_entry_seqnum === 0n) this.header.head_entry_seqnum = this.nextSeqnum;
    if (this.header.head_entry_realtime === 0n) this.header.head_entry_realtime = realtime;
    this.header.tail_entry_seqnum = this.nextSeqnum;
    this.header.tail_entry_realtime = realtime;
    this.header.tail_entry_monotonic = monotonic;
    this.header.tail_entry_boot_id = Buffer.from(bootId);
    this.header.tail_entry_offset = entryOffset;
    this.nextSeqnum++;
  }

  _publishObjectMetadata() {
    this._writeUint64At(96n, this.header.arena_size);
    this._writeUint64At(136n, this.header.tail_object_offset);
    this._writeUint64At(144n, this.header.n_objects);
    this._writeUint64At(208n, this.header.n_data);
    this._writeUint64At(216n, this.header.n_fields);
    this._writeUint64At(232n, this.header.n_entry_arrays);
    this._writeUint64At(240n, this.header.data_hash_chain_depth);
    this._writeUint64At(248n, this.header.field_hash_chain_depth);
  }

  _publishEntryMetadata() {
    this._writeUUIDAt(56n, this.header.tail_entry_boot_id);
    this._writeUint64At(160n, this.header.tail_entry_seqnum);
    this._writeUint64At(168n, this.header.head_entry_seqnum);
    this._writeUint64At(176n, this.header.entry_array_offset);
    this._writeUint64At(184n, this.header.head_entry_realtime);
    this._writeUint64At(192n, this.header.tail_entry_realtime);
    this._writeUint64At(200n, this.header.tail_entry_monotonic);
    this._writeUint32At(256n, this.header.tail_entry_array_offset);
    this._writeUint32At(260n, this.header.tail_entry_array_n_entries);
    this._writeUint64At(264n, this.header.tail_entry_offset);
    // n_entries last (makes entry visible to live readers)
    this._writeUint64At(152n, this.header.n_entries);
  }

  _nextEntryArrayCapacity(index, previousCapacity) {
    let capacity = previousCapacity;
    if (index > capacity) capacity = (index + 1n) * 2n;
    else capacity *= 2n;
    return capacity < 4n ? 4n : capacity;
  }

  _appendToEntryArray(entryOffset) {
    if (this.header.entry_array_offset === 0n) {
      const arrayOff = this._allocateOffsetArray(4n);
      this.header.entry_array_offset = arrayOff;
      this.header.tail_entry_array_offset = Number(arrayOff);
      this.header.tail_entry_array_n_entries = 1;
      this._writeArrayItem(arrayOff, 0n, entryOffset);
      return;
    }

    let tailOffset = BigInt(this.header.tail_entry_array_offset);
    if (tailOffset === 0n) {
      tailOffset = this.header.entry_array_offset;
      let remaining = this.header.n_entries;
      for (;;) {
        const { capacity, nextOffset } = this._readOffsetArrayHeader(tailOffset);
        if (remaining < BigInt(capacity) || nextOffset === 0n) break;
        remaining -= BigInt(capacity);
        tailOffset = nextOffset;
      }
    }

    const { capacity } = this._readOffsetArrayHeader(tailOffset);
    let tailEntries = BigInt(this.header.tail_entry_array_n_entries);
    if (tailEntries === 0n) {
      tailEntries = this.header.n_entries;
      let offset = this.header.entry_array_offset;
      while (offset !== 0n && offset !== tailOffset) {
        const { capacity: c, nextOffset } = this._readOffsetArrayHeader(offset);
        tailEntries -= BigInt(c);
        offset = nextOffset;
      }
    }

    if (tailEntries < BigInt(capacity)) {
      this._writeArrayItem(tailOffset, tailEntries, entryOffset);
      this.header.tail_entry_array_offset = Number(tailOffset);
      this.header.tail_entry_array_n_entries = Number(tailEntries + 1n);
      return;
    }

    const newOff = this._allocateOffsetArray(this._nextEntryArrayCapacity(this.header.n_entries, BigInt(capacity)));
    this._writeUint64At(tailOffset + 16n, newOff);
    this._writeArrayItem(newOff, 0n, entryOffset);
    this.header.tail_entry_array_offset = Number(newOff);
    this.header.tail_entry_array_n_entries = 1;
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
    this.header.n_entry_arrays++;
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
      const arrayOff = this._allocateOffsetArray(4n);
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
        const newOff = this._allocateOffsetArray(this._nextEntryArrayCapacity(currentCount, BigInt(capacity)));
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
    this._closeWithState(STATE_ONLINE);
  }

  closeOffline() {
    this._closeWithState(STATE_OFFLINE);
  }

  _closeWithState(state) {
    if (this.closed) return;
    let closeError = null;
    try {
      this.header.state = state;
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
    try {
      this.lock.release();
    } catch (error) {
      if (!closeError) closeError = error;
    }
    this.lock = null;
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
      try {
        this.lock.release();
      } catch (error) {
        if (!closeError) closeError = error;
      }
      this.lock = null;
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

function normalizeCompression(value) {
  if (value === undefined || value === null || value === COMPRESSION_NONE || value === 'none') {
    return COMPRESSION_NONE;
  }
  if (value === COMPRESSION_ZSTD || value === 'zstd') {
    return COMPRESSION_ZSTD;
  }
  if (value === COMPRESSION_LZ4 || value === 'lz4') {
    return COMPRESSION_LZ4;
  }
  throw new Error(`unsupported compression: ${value}`);
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
