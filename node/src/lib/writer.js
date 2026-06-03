// Journal file writer. Creates regular-by-default keyed-hash journal files.
// Default options are compatible with stock journalctl readers during live append.

import { writeSync, readSync, closeSync, ftruncateSync, fsyncSync } from 'node:fs';
import { zstdCompressSync } from 'node:zlib';
import { readUint64LE, writeUint64LE, writeUint32LE, align8, randomUUID, isZeroUUID, bufEqual, stringToUUID } from './binary.js';
import { safeOpenSync, safeRenameSync } from './fs-safe.js';
import {
  serializeFileHeader, parseObjectHeader, writeObjectHeader,
  HEADER_SIZE, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_FIELD,
  STATE_OFFLINE, STATE_ONLINE, STATE_ARCHIVED,
  INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_LZ4, INCOMPATIBLE_COMPACT,
  COMPATIBLE_TAIL_ENTRY_BOOT_ID,
  OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
  FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
  COMPACT_ENTRY_ITEM_SIZE, COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
  COMPACT_DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_TAIL_OFFSET_OFFSET,
  COMPACT_DATA_TAIL_ENTRIES_OFFSET, JOURNAL_COMPACT_SIZE_MAX,
  DEFAULT_FIELD_HASH_BUCKETS,
  normalizeJournalMaxFileSize, dataHashBucketsForMaxFileSize,
  FILE_SIZE_INCREASE,
} from './header.js';
import { SealState, TAG_LENGTH, OBJECT_TYPE_TAG, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS } from './seal.js';
import { sipHash24, jenkinsHash64 } from './hash.js';
import { decompressZstdDataPayload } from './compress.js';
import { compressLz4DataPayload, decompressLz4DataPayload } from './lz4-block.js';
import { compressXzDataPayload, decompressXzDataPayload } from './xz-block.js';
import {
  fieldCacheKey,
  openedMonotonicBaseMs,
  readAppendHeaderFromFd,
  readObjectHeaderFromFd,
  readObjectSizeFromFd,
  syncParentDirectory,
  validateAppendHeaderForWrite,
} from './writer-file.js';
import {
  FIELD_NAME_POLICY_JOURNALD,
  fieldNameBytes,
  normalizeFieldNamePolicy,
  prepareFieldsForPolicy,
  prepareRawPayloadsForPolicy,
} from './writer-policy.js';

export const COMPRESSION_NONE = 0;
export const COMPRESSION_ZSTD = 1;
export const COMPRESSION_XZ = 2;
export const COMPRESSION_LZ4 = 3;
export const DEFAULT_COMPRESS_THRESHOLD = 512;
export const MIN_COMPRESS_THRESHOLD = 8;
export {
  FIELD_NAME_POLICY_JOURNALD,
  FIELD_NAME_POLICY_RAW,
  FIELD_NAME_POLICY_JOURNAL_APP,
  fieldNameBytes,
  normalizeFieldNamePolicy,
  prepareFieldsForPolicy,
  prepareRawPayloadsForPolicy,
  validateFieldNameForPolicy,
  writerPolicyForLogPolicy,
} from './writer-policy.js';
const FIELD_CACHE_MAX_ENTRIES = 1024;

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
    this.compression = COMPRESSION_NONE;
    this.compressThreshold = DEFAULT_COMPRESS_THRESHOLD;
    this.compact = false;
    this.seal = null;
    this.livePublishEveryEntries = 1;
    this.entriesSinceLivePublication = 0;
    this.fieldNamePolicy = FIELD_NAME_POLICY_JOURNALD;
    this.fieldCache = new Map();
  }

  // Create or truncate a journal file.
  static create(path, opts = {}) {
    let fd;
    try {
      fd = safeOpenSync(path, 'w+', 0o640);
      ftruncateSync(fd, 0);
      const w = new Writer(fd, path);
      w.compression = normalizeCompression(opts.compression);
      w.compressThreshold = normalizeCompressThreshold(opts.compressionThresholdBytes);
      w.compact = opts.compact === true || opts.format === 'compact';
      w.livePublishEveryEntries = normalizeLivePublishEveryEntries(opts.livePublishEveryEntries ?? opts.live_publish_every_entries);
      w.fieldNamePolicy = normalizeFieldNamePolicy(opts.fieldNamePolicy ?? opts.field_name_policy);
      if (opts.seal) {
        w.seal = new SealState(opts.seal);
      }
      w._initialize(opts);
      return w;
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      throw error;
    }
  }

  // Open an existing journal file for appending.
  static open(path, opts = {}) {
    let fd;
    try {
      fd = safeOpenSync(path, 'r+');
      const header = readAppendHeaderFromFd(fd);
      validateAppendHeaderForWrite(header);
      const tailSize = readObjectSizeFromFd(fd, header.tail_object_offset);
      const w = new Writer(fd, path);
      w._configureOpenAppendState(header, opts, tailSize);
      w.header.state = STATE_ONLINE;
      w._writeHeader();
      return w;
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      throw error;
    }
  }

  _configureOpenAppendState(header, opts, tailSize) {
    this.header = header;
    this.appendOffset = align8(header.tail_object_offset + tailSize);
    this.nextSeqnum = header.tail_entry_seqnum + 1n;
    this.bootId = this._openedBootId(header, opts);
    this.started = Date.now() - openedMonotonicBaseMs(header);
    this.compression = openedCompressionFromHeader(header);
    this.compressThreshold = DEFAULT_COMPRESS_THRESHOLD;
    this.compact = (header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;
    this.livePublishEveryEntries = normalizeLivePublishEveryEntries(opts.livePublishEveryEntries ?? opts.live_publish_every_entries);
    this.fieldNamePolicy = normalizeFieldNamePolicy(opts.fieldNamePolicy ?? opts.field_name_policy);
  }

  _openedBootId(header, opts) {
    const bootId = Buffer.from(header.tail_entry_boot_id);
    if (!isZeroUUID(bootId)) return bootId;
    const explicitBootId = uuidOption(opts.bootId ?? opts.boot_id, 'boot id');
    return explicitBootId && !isZeroUUID(explicitBootId) ? explicitBootId : Buffer.from(header.file_id);
  }

  _initialize(opts) {
    const maxFileSize = normalizeJournalMaxFileSize(opts.maxFileSize ?? opts.max_file_size, this.compact);
    const dataBuckets = opts.dataHashTableBuckets || opts.data_hash_table_buckets || dataHashBucketsForMaxFileSize(maxFileSize);
    const fieldBuckets = opts.fieldHashTableBuckets || opts.field_hash_table_buckets || DEFAULT_FIELD_HASH_BUCKETS;

    const dataSize = BigInt(dataBuckets * HASH_ITEM_SIZE);
    const fieldSize = BigInt(fieldBuckets * HASH_ITEM_SIZE);
    // systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE
    const fieldObjOffset = BigInt(HEADER_SIZE);
    const fieldOffset = fieldObjOffset + BigInt(OBJECT_HEADER_SIZE);
    const dataObjOffset = align8(fieldOffset + fieldSize);
    const dataOffset = dataObjOffset + BigInt(OBJECT_HEADER_SIZE);
    const appendOffset = align8(dataOffset + dataSize);
    const increment = BigInt(FILE_SIZE_INCREASE);
    const fileSize = ((appendOffset + increment - 1n) / increment) * increment;
    if (this.compact && fileSize > JOURNAL_COMPACT_SIZE_MAX) {
      throw new Error('compact journal cannot exceed 4 GiB');
    }
    if (fileSize > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error('journal file offset exceeds JavaScript safe integer range');
    }

    const fileId = uuidOption(opts.fileId ?? opts.file_id, 'file id') || randomUUID();
    const machineId = uuidOption(opts.machineId ?? opts.machine_id, 'machine id') || randomUUID();
    const bootId = uuidOption(opts.bootId ?? opts.boot_id, 'boot id') || randomUUID();
    const seqnumId = uuidOption(opts.seqnumId ?? opts.seqnum_id, 'seqnum id') || randomUUID();

    let incFlags = INCOMPATIBLE_KEYED_HASH;
    if (this.compression === COMPRESSION_XZ) {
      incFlags |= INCOMPATIBLE_COMPRESSED_XZ;
    } else if (this.compression === COMPRESSION_ZSTD) {
      incFlags |= INCOMPATIBLE_COMPRESSED_ZSTD;
    } else if (this.compression === COMPRESSION_LZ4) {
      incFlags |= INCOMPATIBLE_COMPRESSED_LZ4;
    }
    if (this.compact) incFlags |= INCOMPATIBLE_COMPACT;

    let compatibleFlags = COMPATIBLE_TAIL_ENTRY_BOOT_ID;
    if (this.seal) {
      compatibleFlags |= COMPATIBLE_SEALED | COMPATIBLE_SEALED_CONTINUOUS;
    }

    this.header = {
      signature: 'LPKSHHRH',
      compatible_flags: compatibleFlags,
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

    if (this.seal) {
      this._appendFirstTag();
    }
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

    const payloads = [];
    const preparedFields = prepareFieldsForPolicy(fields, this.fieldNamePolicy);
    for (const field of preparedFields) {
      const name = field.name;
      const nameBuf = fieldNameBytes(name);
      const valueBuf = Buffer.isBuffer(field.value) ? field.value : Buffer.from(field.value);
      const payload = Buffer.alloc(nameBuf.length + 1 + valueBuf.length);
      nameBuf.copy(payload, 0);
      payload[nameBuf.length] = 0x3d;
      valueBuf.copy(payload, nameBuf.length + 1);
      payloads.push(payload);
    }
    return this._appendPayloads(payloads, opts);
  }

  // Append one entry from complete KEY=value byte payloads.
  appendRaw(payloads, opts = {}) {
    if (this.closed) throw new Error('writer closed');
    return this._appendPayloads(prepareRawPayloadsForPolicy(payloads, this.fieldNamePolicy), opts);
  }

  _appendPayloads(payloads, opts = {}) {
    if (payloads.length === 0) throw new Error('empty entry');

    const now = Date.now();
    const hasRealtime = Object.prototype.hasOwnProperty.call(opts, 'realtimeUsec');
    const hasMonotonic = Object.prototype.hasOwnProperty.call(opts, 'monotonicUsec');
    const realtime = hasRealtime ? BigInt(opts.realtimeUsec) : BigInt(now * 1000);
    const monotonic = hasMonotonic ? BigInt(opts.monotonicUsec) : BigInt((now - this.started) * 1000);
    const explicitBootId = uuidOption(opts.bootId ?? opts.boot_id, 'entry boot id');
    const bootId = explicitBootId && !isZeroUUID(explicitBootId) ? explicitBootId : this.bootId;

    this._maybeAppendTag(realtime);

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
    const entryItemSize = this._entryItemSize();
    const entrySize = BigInt(ENTRY_OBJECT_HEADER_SIZE + deduped.length * entryItemSize);
    this._ensureCompactObjectFits(entryOffset, entrySize);
    const alignedSize = align8(entrySize);
    const entryBuf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(entryBuf, 0, OBJECT_TYPE_ENTRY, 0, entrySize);
    writeUint64LE(entryBuf, 16, this.nextSeqnum);
    writeUint64LE(entryBuf, 24, realtime);
    writeUint64LE(entryBuf, 32, monotonic);
    bootId.copy(entryBuf, 40);
    writeUint64LE(entryBuf, 56, xorHash);
    for (let i = 0; i < deduped.length; i++) {
      const off = ENTRY_OBJECT_HEADER_SIZE + i * entryItemSize;
      if (this.compact) {
        this._ensureCompactOffset(deduped[i].offset);
        entryBuf.writeUInt32LE(Number(deduped[i].offset), off);
      } else {
        writeUint64LE(entryBuf, off, deduped[i].offset);
        writeUint64LE(entryBuf, off + 8, deduped[i].hash);
      }
    }
    writeSync(this.fd, entryBuf, 0, entryBuf.length, Number(this.appendOffset));
    this._objectAdded(entryOffset, entrySize);

    // Publish object reachability before entry count
    this._publishObjectMetadata();
    this._hmacPutObject(entryOffset, OBJECT_TYPE_ENTRY);

    // Append to entry array and link data
    this._appendToEntryArray(entryOffset);
    for (const item of deduped) this._linkDataToEntry(item.offset, entryOffset);

    // Commit entry metadata last (so live readers see complete rows)
    this._entryAdded(entryOffset, realtime, monotonic, bootId);
    this._publishEntryMetadata();
    this._publishAfterEntry();

    return { realtime, seqnum: this.nextSeqnum - 1n };
  }

  // Append a string-valued map with sorted keys.
  appendMap(fieldsMap) {
    const keys = Object.keys(fieldsMap).sort();
    return this.append(keys.map(k => ({ name: k, value: Reflect.get(fieldsMap, k) })));
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
    } else if (this.compression === COMPRESSION_XZ && payload.length >= this.compressThreshold) {
      try {
        const compressed = compressXzDataPayload(payload);
        if (compressed && compressed.length < payload.length) {
          objectPayload = compressed;
          compressionFlag = OBJECT_COMPRESSED_XZ;
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

    const payloadOffset = this._dataPayloadOffset();
    const size = BigInt(payloadOffset + objectPayload.length);
    this._ensureCompactObjectFits(offset, size);
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_DATA, compressionFlag, size);
    writeUint64LE(buf, 16, hash);
    objectPayload.copy(buf, payloadOffset);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);

    // Insert into data hash table
    this._appendHashItem(this.header.data_hash_table_offset, this.header.data_hash_table_size, OBJECT_TYPE_DATA, hash, offset);
    this.header.n_data++;
    this._hmacPutObject(offset, OBJECT_TYPE_DATA);

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
    const cacheKey = fieldCacheKey(payload);
    const cached = cacheKey === null ? undefined : this.fieldCache.get(cacheKey);
    if (cached !== undefined) return cached;
    const hash = this._hash(payload);
    const existing = this._findField(hash, payload);
    if (existing !== null) {
      this._cacheField(payload, existing);
      return existing;
    }

    const offset = this.appendOffset;
    const size = BigInt(FIELD_OBJECT_HEADER_SIZE + payload.length);
    this._ensureCompactObjectFits(offset, size);
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
    this._hmacPutObject(offset, OBJECT_TYPE_FIELD);

    this._cacheField(payload, offset);
    return offset;
  }

  _cacheField(payload, offset) {
    const cacheKey = fieldCacheKey(payload);
    if (cacheKey === null) return;
    if (this.fieldCache.size >= FIELD_CACHE_MAX_ENTRIES && !this.fieldCache.has(cacheKey)) {
      this.fieldCache.clear();
    }
    this.fieldCache.set(cacheKey, offset);
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
    const allowedCompressionFlags = OBJECT_COMPRESSED_ZSTD | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_XZ;
    const compressionFlags = objHeader.flags & allowedCompressionFlags;
    if ((objHeader.flags & ~allowedCompressionFlags) !== 0) {
      throw new Error(`unsupported DATA object flags: 0x${objHeader.flags.toString(16)}`);
    }
    if (compressionFlags !== 0 && (compressionFlags & (compressionFlags - 1)) !== 0) {
      throw new Error(`unsupported DATA object compression flags: 0x${objHeader.flags.toString(16)}`);
    }
    const objSize = objHeader.size;
    const payloadOffset = this._dataPayloadOffset();
    const payloadLen = Number(objSize) - payloadOffset;
    if (payloadLen <= 0) return null;
    const buf = Buffer.alloc(payloadLen);
    readSync(this.fd, buf, 0, payloadLen, Number(offset) + payloadOffset);
    if ((objHeader.flags & OBJECT_COMPRESSED_ZSTD) !== 0) {
      return decompressZstdDataPayload(buf);
    }
    if ((objHeader.flags & OBJECT_COMPRESSED_LZ4) !== 0) {
      return decompressLz4DataPayload(buf);
    }
    if ((objHeader.flags & OBJECT_COMPRESSED_XZ) !== 0) {
      return decompressXzDataPayload(buf);
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
    this._ensureArenaSize(this.appendOffset);
  }

  _ensureArenaSize(requiredSize) {
    const currentSize = BigInt(HEADER_SIZE) + this.header.arena_size;
    if (requiredSize <= currentSize) return;
    const increment = BigInt(FILE_SIZE_INCREASE);
    const newSize = ((requiredSize + increment - 1n) / increment) * increment;
    if (this.compact && newSize > JOURNAL_COMPACT_SIZE_MAX) {
      throw new Error('compact journal cannot exceed 4 GiB');
    }
    if (newSize > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error('journal file offset exceeds JavaScript safe integer range');
    }
    this.header.arena_size = newSize - BigInt(HEADER_SIZE);
    ftruncateSync(this.fd, Number(newSize));
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

  _postChange() {
    const size = this.header.header_size + this.header.arena_size;
    if (size > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error('journal file offset exceeds JavaScript safe integer range');
    }
    ftruncateSync(this.fd, Number(size));
  }

  _publishAfterEntry() {
    if (this.livePublishEveryEntries === 0) return;
    if (this.livePublishEveryEntries === 1) {
      this._postChange();
      return;
    }
    this.entriesSinceLivePublication++;
    if (this.entriesSinceLivePublication >= this.livePublishEveryEntries) {
      this.entriesSinceLivePublication = 0;
      this._postChange();
    }
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
    const itemSize = this._offsetArrayItemSize();
    if ((oh.size - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) % BigInt(itemSize) !== 0n) {
      throw new Error('invalid entry array object size');
    }
    const capacity = Number((oh.size - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) / BigInt(itemSize));
    const nextOffset = readUint64LE(buf, 16);
    return { capacity, nextOffset };
  }

  _allocateOffsetArray(capacity) {
    const offset = this.appendOffset;
    const size = BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE) + capacity * BigInt(this._offsetArrayItemSize());
    this._ensureCompactObjectFits(offset, size);
    const alignedSize = align8(size);
    const buf = Buffer.alloc(Number(alignedSize));
    writeObjectHeader(buf, 0, OBJECT_TYPE_ENTRY_ARRAY, 0, size);
    writeSync(this.fd, buf, 0, buf.length, Number(this.appendOffset));
    this._objectAdded(offset, size);
    this.header.n_entry_arrays++;
    this._publishObjectMetadata();
    this._hmacPutObject(offset, OBJECT_TYPE_ENTRY_ARRAY);
    return offset;
  }

  _writeArrayItem(arrayOffset, index, entryOffset) {
    const itemOffset = arrayOffset + BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE) + index * BigInt(this._offsetArrayItemSize());
    if (this.compact) {
      this._ensureCompactOffset(entryOffset);
      this._writeUint32At(itemOffset, Number(entryOffset));
      return;
    }
    this._writeUint64At(itemOffset, entryOffset);
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
      if (this.compact) {
        this._writeUint32At(dataOffset + COMPACT_DATA_TAIL_OFFSET_OFFSET, Number(arrayOff));
        this._writeUint32At(dataOffset + COMPACT_DATA_TAIL_ENTRIES_OFFSET, 1);
      }
      this._writeUint64At(dataOffset + 56n, 2n);
    } else {
      const entryArrayOff = this._readUint64At(dataOffset + 48n);
      if (entryArrayOff === 0n) throw new Error('invalid journal: missing data entry array');
      const { tailOffset, tailEntries } = this._appendToDataEntryArray(entryArrayOff, nEntries - 1n, entryOffset);
      if (this.compact) {
        this._writeUint32At(dataOffset + COMPACT_DATA_TAIL_OFFSET_OFFSET, Number(tailOffset));
        this._writeUint32At(dataOffset + COMPACT_DATA_TAIL_ENTRIES_OFFSET, Number(tailEntries));
      }
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
        return { tailOffset: offset, tailEntries: remaining + 1n };
      }
      remaining -= BigInt(capacity);
      if (nextOffset === 0n) {
        const newOff = this._allocateOffsetArray(this._nextEntryArrayCapacity(currentCount, BigInt(capacity)));
        this._writeUint64At(offset + 16n, newOff);
        this._writeArrayItem(newOff, 0n, entryOffset);
        return { tailOffset: newOff, tailEntries: 1n };
      }
      offset = nextOffset;
    }
  }

  _entryItemSize() {
    return this.compact ? COMPACT_ENTRY_ITEM_SIZE : REGULAR_ENTRY_ITEM_SIZE;
  }

  _offsetArrayItemSize() {
    return this.compact ? COMPACT_OFFSET_ARRAY_ITEM_SIZE : REGULAR_OFFSET_ARRAY_ITEM_SIZE;
  }

  _dataPayloadOffset() {
    return this.compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
  }

  _ensureCompactOffset(offset) {
    if (this.compact && offset > JOURNAL_COMPACT_SIZE_MAX) {
      throw new Error('compact journal offset exceeds 32-bit range');
    }
  }

  _ensureCompactObjectFits(offset, size) {
    if (!this.compact) return;
    if (offset > JOURNAL_COMPACT_SIZE_MAX || align8(offset + size) > JOURNAL_COMPACT_SIZE_MAX) {
      throw new Error('compact journal cannot exceed 4 GiB');
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
    this.closed = true;
    if (closeError) throw closeError;
  }

  archiveTo(path) {
    if (this.closed) throw new Error('writer closed');
    this.header.state = STATE_ARCHIVED;
    this._writeHeader();
    fsyncSync(this.fd);
    try {
      if (this.path !== path) {
        safeRenameSync(this.path, path);
      }
      this.path = path;
      let closeError = null;
      try {
        syncParentDirectory(path);
      } catch (error) {
        closeError = error;
      }
      try {
        closeSync(this.fd);
        this.closed = true;
      } catch (error) {
        if (!closeError) closeError = error;
      }
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

  // Sealing methods

  _appendTag() {
    if (!this.seal) return;
    this.seal.hmacStart();
    const offset = this.appendOffset;
    const size = BigInt(OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH);
    const seqnum = this.header.n_tags + 1n;
    const epoch = this.seal.getEpoch();
    const buf = Buffer.alloc(Number(align8(size)));
    writeObjectHeader(buf, 0, OBJECT_TYPE_TAG, 0, size);
    writeUint64LE(buf, OBJECT_HEADER_SIZE, seqnum);
    writeUint64LE(buf, OBJECT_HEADER_SIZE + 8, epoch);
    this.seal.hmacWrite(buf.slice(0, OBJECT_HEADER_SIZE + 16));
    buf.slice(OBJECT_HEADER_SIZE + 16, OBJECT_HEADER_SIZE + 16 + TAG_LENGTH)
      .set(this.seal.hmacSum());
    writeSync(this.fd, buf, 0, buf.length, Number(offset));
    this._objectAdded(offset, size);
    this.header.n_tags = seqnum;
    this.seal.hmacReset();
  }

  _appendFirstTag() {
    if (!this.seal) return;
    this._hmacPutHeader();
    this._hmacPutHashTableObject(this.header.field_hash_table_offset - BigInt(OBJECT_HEADER_SIZE));
    this._hmacPutHashTableObject(this.header.data_hash_table_offset - BigInt(OBJECT_HEADER_SIZE));
    this._appendTag();
  }

  _maybeAppendTag(realtime) {
    if (!this.seal) return;
    const need = this.seal.needEvolve(realtime);
    if (!need) return;
    this._appendTag();
    for (;;) {
      const goal = this.seal.getGoalEpoch(realtime);
      const epoch = this.seal.getEpoch();
      if (epoch >= goal) break;
      this.seal.evolveState();
      if (this.seal.getEpoch() < goal) {
        this._appendTag();
      }
    }
  }

  _hmacPutHeader() {
    if (!this.seal) return;
    this.seal.hmacStart();
    const buf = Buffer.alloc(HEADER_SIZE);
    serializeFileHeader(buf, this.header);
    this.seal.hmacWrite(buf.slice(0, 16));
    this.seal.hmacWrite(buf.slice(24, 56));
    this.seal.hmacWrite(buf.slice(72, 96));
    this.seal.hmacWrite(buf.slice(104, 136));
  }

  _hmacPutHashTableObject(objectStart) {
    if (!this.seal) return;
    this.seal.hmacStart();
    const buf = Buffer.alloc(OBJECT_HEADER_SIZE);
    readSync(this.fd, buf, 0, OBJECT_HEADER_SIZE, Number(objectStart));
    this.seal.hmacWrite(buf);
  }

  _hmacPutObject(objectStart, typ) {
    if (!this.seal) return;
    this.seal.hmacStart();
    const headerBuf = Buffer.alloc(OBJECT_HEADER_SIZE);
    readSync(this.fd, headerBuf, 0, OBJECT_HEADER_SIZE, Number(objectStart));
    this.seal.hmacWrite(headerBuf);
    const objSize = readUint64LE(headerBuf, 8);
    switch (typ) {
      case OBJECT_TYPE_DATA: {
        const hashBuf = Buffer.alloc(8);
        readSync(this.fd, hashBuf, 0, 8, Number(objectStart) + 16);
        this.seal.hmacWrite(hashBuf);
        const payloadOffset = this._dataPayloadOffset();
        const payloadSize = Number(objSize) - payloadOffset;
        if (payloadSize > 0) {
          const payload = Buffer.alloc(payloadSize);
          readSync(this.fd, payload, 0, payloadSize, Number(objectStart) + payloadOffset);
          this.seal.hmacWrite(payload);
        }
        break;
      }
      case OBJECT_TYPE_FIELD: {
        const hashBuf = Buffer.alloc(8);
        readSync(this.fd, hashBuf, 0, 8, Number(objectStart) + 16);
        this.seal.hmacWrite(hashBuf);
        const payloadSize = Number(objSize) - FIELD_OBJECT_HEADER_SIZE;
        if (payloadSize > 0) {
          const payload = Buffer.alloc(payloadSize);
          readSync(this.fd, payload, 0, payloadSize, Number(objectStart) + FIELD_OBJECT_HEADER_SIZE);
          this.seal.hmacWrite(payload);
        }
        break;
      }
      case OBJECT_TYPE_ENTRY: {
        const restSize = Number(objSize) - OBJECT_HEADER_SIZE;
        if (restSize > 0) {
          const rest = Buffer.alloc(restSize);
          readSync(this.fd, rest, 0, restSize, Number(objectStart) + OBJECT_HEADER_SIZE);
          this.seal.hmacWrite(rest);
        }
        break;
      }
      case OBJECT_TYPE_DATA_HASH_TABLE:
      case OBJECT_TYPE_FIELD_HASH_TABLE:
      case OBJECT_TYPE_ENTRY_ARRAY:
        break;
      case OBJECT_TYPE_TAG: {
        const meta = Buffer.alloc(16);
        readSync(this.fd, meta, 0, 16, Number(objectStart) + OBJECT_HEADER_SIZE);
        this.seal.hmacWrite(meta);
        break;
      }
    }
  }
}

function openedCompressionFromHeader(header) {
  if ((header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ) !== 0) return COMPRESSION_XZ;
  if ((header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4) !== 0) return COMPRESSION_LZ4;
  if ((header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD) !== 0) return COMPRESSION_ZSTD;
  return COMPRESSION_NONE;
}

function normalizeCompression(value) {
  if (value === undefined || value === null || value === COMPRESSION_NONE || value === 'none') {
    return COMPRESSION_NONE;
  }
  if (value === COMPRESSION_XZ || value === 'xz') {
    return COMPRESSION_XZ;
  }
  if (value === COMPRESSION_ZSTD || value === 'zstd') {
    return COMPRESSION_ZSTD;
  }
  if (value === COMPRESSION_LZ4 || value === 'lz4') {
    return COMPRESSION_LZ4;
  }
  throw new Error(`unsupported compression: ${value}`);
}

function normalizeCompressThreshold(value) {
  if (value === undefined || value === null) return DEFAULT_COMPRESS_THRESHOLD;
  if (!Number.isSafeInteger(value)) {
    throw new Error(`invalid compression threshold: ${value}`);
  }
  return Math.max(MIN_COMPRESS_THRESHOLD, value);
}

function normalizeLivePublishEveryEntries(value) {
  if (value === undefined || value === null) return 1;
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`invalid livePublishEveryEntries: ${value}`);
  }
  return value;
}

function uuidOption(value, label) {
  if (value === undefined || value === null) return null;
  let out;
  if (typeof value === 'string') {
    const clean = value.trim().replaceAll('-', '');
    if (!/^[0-9a-fA-F]{32}$/.test(clean)) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
    out = stringToUUID(clean);
  } else {
    out = Buffer.from(value);
  }
  if (out.length !== 16) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
  return out;
}
