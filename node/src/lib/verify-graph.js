// Raw object-graph verification for systemd journal files.

import {
  COMPATIBLE_SEALED,
  COMPATIBLE_SEALED_CONTINUOUS,
  COMPATIBLE_TAIL_ENTRY_BOOT_ID,
  COMPACT_DATA_OBJECT_HEADER_SIZE,
  COMPACT_ENTRY_ITEM_SIZE,
  COMPACT_OFFSET_ARRAY_ITEM_SIZE,
  DATA_OBJECT_HEADER_SIZE,
  ENTRY_OBJECT_HEADER_SIZE,
  FIELD_OBJECT_HEADER_SIZE,
  HASH_ITEM_SIZE,
  HEADER_MIN_SIZE,
  INCOMPATIBLE_COMPACT,
  INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_COMPRESSED_ZSTD,
  INCOMPATIBLE_KEYED_HASH,
  JOURNAL_COMPACT_SIZE_MAX,
  OBJECT_COMPRESSED_LZ4,
  OBJECT_COMPRESSED_XZ,
  OBJECT_COMPRESSED_ZSTD,
  OBJECT_HEADER_SIZE,
  OBJECT_TYPE_DATA,
  OBJECT_TYPE_DATA_HASH_TABLE,
  OBJECT_TYPE_ENTRY,
  OBJECT_TYPE_ENTRY_ARRAY,
  OBJECT_TYPE_FIELD,
  OBJECT_TYPE_FIELD_HASH_TABLE,
  OBJECT_TYPE_TAG,
  OFFSET_ARRAY_OBJECT_HEADER_SIZE,
  REGULAR_ENTRY_ITEM_SIZE,
  REGULAR_OFFSET_ARRAY_ITEM_SIZE,
  parseFileHeader,
} from './header.js';
import { decompressZstdDataPayload } from './compress.js';
import { jenkinsHash64, sipHash24 } from './hash.js';
import { decompressLz4DataPayload } from './lz4-block.js';
import { decompressXzDataPayload } from './xz-block.js';

const OBJECT_TYPES = new Map([
  [OBJECT_TYPE_DATA, 'DATA'],
  [OBJECT_TYPE_FIELD, 'FIELD'],
  [OBJECT_TYPE_ENTRY, 'ENTRY'],
  [OBJECT_TYPE_DATA_HASH_TABLE, 'DATA_HASH_TABLE'],
  [OBJECT_TYPE_FIELD_HASH_TABLE, 'FIELD_HASH_TABLE'],
  [OBJECT_TYPE_ENTRY_ARRAY, 'ENTRY_ARRAY'],
  [OBJECT_TYPE_TAG, 'TAG'],
]);
const OBJECT_COMPRESSED_MASK = OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD;
const COMPATIBLE_SUPPORTED_MASK = COMPATIBLE_SEALED | COMPATIBLE_TAIL_ENTRY_BOOT_ID | COMPATIBLE_SEALED_CONTINUOUS;
const TAG_OBJECT_SIZE = OBJECT_HEADER_SIZE + 8 + 8 + 32;

export class ObjectGraphVerificationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ObjectGraphVerificationError';
  }
}

export function verifyObjectGraph(data) {
  new GraphVerifier(data).verify();
}

class GraphVerifier {
  constructor(data) {
    this.data = data;
    this.header = null;
    this.compact = false;
    this.spans = new Map();
    this.order = [];
    this.dataObjects = new Map();
    this.fieldObjects = new Map();
    this.entryObjects = new Map();
    this.entryArrays = new Map();
    this.counts = new Map();
    this.mainEntryArrayFound = false;
  }

  verify() {
    this.readHeader();
    this.walkObjects();
    this.validateHeaderCounts();
    this.validateMainEntryArrayPresence();
    this.validateTailMetadata();
    this.validateGlobalEntryArray();
    this.validateDataHashTable();
  }

  readHeader() {
    if (this.data.length < HEADER_MIN_SIZE) {
      throw new ObjectGraphVerificationError('file too small');
    }
    this.header = parseFileHeader(this.data);
    this.compact = (this.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;

    const headerSize = this.hnum('header_size');
    const arenaSize = this.hnum('arena_size');
    if (headerSize < HEADER_MIN_SIZE) {
      throw new ObjectGraphVerificationError(`invalid header_size ${headerSize}`);
    }
    if (headerSize > this.data.length) {
      throw new ObjectGraphVerificationError(`header_size ${headerSize} exceeds file size`);
    }
    if (headerSize % 8 !== 0) {
      throw new ObjectGraphVerificationError(`header_size ${headerSize} is not aligned`);
    }
    if (headerSize + arenaSize > this.data.length) {
      throw new ObjectGraphVerificationError('header_size + arena_size exceeds file size');
    }
    if (![0, 1, 2].includes(this.header.state)) {
      throw new ObjectGraphVerificationError(`invalid journal state ${this.header.state}`);
    }
    if (this.header.compatible_flags & ~COMPATIBLE_SUPPORTED_MASK) {
      throw new ObjectGraphVerificationError(
        `unsupported compatible flags 0x${this.header.compatible_flags.toString(16)}`,
      );
    }
    for (let i = 17; i < 24; i++) {
      if (this.data[i] !== 0) throw new ObjectGraphVerificationError('reserved header bytes are non-zero');
    }
    if (this.compact && BigInt(this.data.length) > JOURNAL_COMPACT_SIZE_MAX) {
      throw new ObjectGraphVerificationError('compact journal exceeds 32-bit size limit');
    }
  }

  walkObjects() {
    const tail = this.hnum('tail_object_offset');
    if (!this.validateObjectWalkTail(tail)) return;
    const state = this.newObjectWalkState();
    let offset = this.hnum('header_size');

    for (;;) {
      const frame = this.readObjectFrame(offset, tail);
      this.recordObjectFrame(frame);
      this.validateObjectCompression(frame);
      this.processObjectFrame(frame, state);

      if (offset === tail) break;
      offset = frame.end;
    }

    this.validateObjectWalkCompletion(tail, state);
  }

  validateObjectWalkTail(tail) {
    if (tail === 0) {
      if (this.header.n_objects !== 0n) {
        throw new ObjectGraphVerificationError('tail_object_offset is zero with objects recorded');
      }
      return false;
    }
    if (tail < this.hnum('header_size')) {
      throw new ObjectGraphVerificationError('tail_object_offset is before header_size');
    }
    return true;
  }

  newObjectWalkState() {
    return {
      entrySeqnum: 0n,
      entrySeqnumSet: false,
      entryMonotonic: 0n,
      entryMonotonicSet: false,
      entryBootID: Buffer.alloc(16),
      entryRealtime: 0n,
      entryRealtimeSet: false,
      lastTagRealtime: 0n,
    };
  }

  readObjectFrame(offset, tail) {
    if (offset > tail) throw new ObjectGraphVerificationError('object walk skipped past tail_object_offset');
    if (offset + OBJECT_HEADER_SIZE > this.data.length) {
      throw new ObjectGraphVerificationError(`object header at offset ${offset} exceeds file bounds`);
    }
    const typ = this.data[offset];
    const flags = this.data[offset + 1];
    const size = this.u64(offset + 8);
    const alignedSize = align8(size);
    const alignedSizeNumber = u64ToNumber(alignedSize, `aligned size at offset ${offset}`);
    const end = offset + alignedSizeNumber;
    this.validateObjectFrame(offset, typ, size, alignedSize, end);
    return { offset, typ, flags, size, end };
  }

  validateObjectFrame(offset, typ, size, alignedSize, end) {
    if (typ === 0 && size === 0n) throw new ObjectGraphVerificationError(`zero object before tail at ${offset}`);
    if (!OBJECT_TYPES.has(typ)) throw new ObjectGraphVerificationError(`unknown object type ${typ} at offset ${offset}`);
    if (size < BigInt(OBJECT_HEADER_SIZE)) {
      throw new ObjectGraphVerificationError(`object size ${size} too small at offset ${offset}`);
    }
    if (alignedSize < size || alignedSize === 0n || end > this.data.length) {
      throw new ObjectGraphVerificationError(`object at offset ${offset} exceeds file bounds`);
    }
    if (offset % 8 !== 0) throw new ObjectGraphVerificationError(`object offset ${offset} is not aligned`);
  }

  recordObjectFrame(frame) {
    this.spans.set(frame.offset, { typ: frame.typ, flags: frame.flags, size: frame.size, end: frame.end });
    this.order.push(frame.offset);
    this.counts.set(frame.typ, (this.counts.get(frame.typ) || 0n) + 1n);
  }

  validateObjectCompression(frame) {
    const { offset, typ, flags } = frame;
    if (flags & ~OBJECT_COMPRESSED_MASK) {
      throw new ObjectGraphVerificationError(`object at offset ${offset} has unknown flags 0x${flags.toString(16)}`);
    }
    if (flagCount(flags & OBJECT_COMPRESSED_MASK) > 1) {
      throw new ObjectGraphVerificationError(`object at offset ${offset} has multiple compression flags`);
    }
    if (typ !== OBJECT_TYPE_DATA && flags !== 0) {
      throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} object at offset ${offset} has compression flags`);
    }
    this.validateCompressionHeaderFlags(frame);
  }

  validateCompressionHeaderFlags(frame) {
    const { offset, flags } = frame;
    if ((flags & OBJECT_COMPRESSED_XZ) && !(this.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ)) {
      throw new ObjectGraphVerificationError(`XZ DATA object without matching header flag at offset ${offset}`);
    }
    if ((flags & OBJECT_COMPRESSED_LZ4) && !(this.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4)) {
      throw new ObjectGraphVerificationError(`LZ4 DATA object without matching header flag at offset ${offset}`);
    }
    if ((flags & OBJECT_COMPRESSED_ZSTD) && !(this.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD)) {
      throw new ObjectGraphVerificationError(`ZSTD DATA object without matching header flag at offset ${offset}`);
    }
  }

  processObjectFrame(frame, state) {
    if (frame.typ === OBJECT_TYPE_DATA) return this.parseData(frame.offset, frame.flags, frame.size);
    if (frame.typ === OBJECT_TYPE_FIELD) return this.parseField(frame.offset, frame.size);
    if (frame.typ === OBJECT_TYPE_ENTRY) return this.processEntryFrame(frame, state);
    if (frame.typ === OBJECT_TYPE_DATA_HASH_TABLE || frame.typ === OBJECT_TYPE_FIELD_HASH_TABLE) {
      return this.parseHashTable(frame.offset, frame.typ, frame.size);
    }
    if (frame.typ === OBJECT_TYPE_ENTRY_ARRAY) return this.processEntryArrayFrame(frame);
    if (frame.typ === OBJECT_TYPE_TAG) return this.processTagFrame(frame, state);
    return undefined;
  }

  processEntryFrame(frame, state) {
    const entry = this.parseEntry(frame.offset, frame.size);
    if ((this.header.compatible_flags & COMPATIBLE_SEALED) && this.count(OBJECT_TYPE_TAG) <= 0n) {
      throw new ObjectGraphVerificationError(`first entry before first tag at offset ${frame.offset}`);
    }
    if (entry.realtime < state.lastTagRealtime) {
      throw new ObjectGraphVerificationError(`older entry after newer tag at offset ${frame.offset}`);
    }
    this.validateEntrySeqnum(frame.offset, entry, state);
    this.validateEntryMonotonic(frame.offset, entry, state);
    if (!state.entryRealtimeSet && entry.realtime !== this.header.head_entry_realtime) {
      throw new ObjectGraphVerificationError(`head entry realtime mismatch at offset ${frame.offset}`);
    }
    state.entryRealtime = entry.realtime;
    state.entryRealtimeSet = true;
  }

  validateEntrySeqnum(offset, entry, state) {
    if (!state.entrySeqnumSet && entry.seqnum !== this.header.head_entry_seqnum) {
      throw new ObjectGraphVerificationError(`head entry seqnum mismatch at offset ${offset}`);
    }
    if (state.entrySeqnumSet && state.entrySeqnum >= entry.seqnum) {
      throw new ObjectGraphVerificationError(`entry seqnum out of sync at offset ${offset}`);
    }
    state.entrySeqnum = entry.seqnum;
    state.entrySeqnumSet = true;
  }

  validateEntryMonotonic(offset, entry, state) {
    if (state.entryMonotonicSet && entry.boot_id.equals(state.entryBootID) && state.entryMonotonic > entry.monotonic) {
      throw new ObjectGraphVerificationError(`entry monotonic out of sync at offset ${offset}`);
    }
    state.entryMonotonic = entry.monotonic;
    state.entryBootID = entry.boot_id;
    state.entryMonotonicSet = true;
  }

  processEntryArrayFrame(frame) {
    this.parseEntryArray(frame.offset, frame.size);
    if (BigInt(frame.offset) !== this.header.entry_array_offset) return;
    if (this.mainEntryArrayFound) throw new ObjectGraphVerificationError('more than one main entry array');
    this.mainEntryArrayFound = true;
  }

  processTagFrame(frame, state) {
    if (!(this.header.compatible_flags & COMPATIBLE_SEALED)) throw new ObjectGraphVerificationError('TAG object in unsealed file');
    if (frame.size !== BigInt(TAG_OBJECT_SIZE)) throw new ObjectGraphVerificationError(`invalid TAG size at offset ${frame.offset}`);
    const seqnum = this.u64(frame.offset + 16);
    if (seqnum !== this.count(OBJECT_TYPE_TAG)) throw new ObjectGraphVerificationError(`TAG seqnum mismatch at offset ${frame.offset}`);
    if (state.entryRealtimeSet) state.lastTagRealtime = state.entryRealtime;
  }

  validateObjectWalkCompletion(tail, state) {
    if (this.order[this.order.length - 1] !== tail) {
      throw new ObjectGraphVerificationError('tail_object_offset does not point to walked tail');
    }
    if (state.entrySeqnumSet && state.entrySeqnum !== this.header.tail_entry_seqnum) {
      throw new ObjectGraphVerificationError('tail_entry_seqnum mismatch');
    }
    if (this.tailEntryMonotonicMismatch(state)) {
      throw new ObjectGraphVerificationError('tail_entry_monotonic mismatch');
    }
    if (state.entryRealtimeSet && state.entryRealtime !== this.header.tail_entry_realtime) {
      throw new ObjectGraphVerificationError('tail_entry_realtime mismatch');
    }
  }

  tailEntryMonotonicMismatch(state) {
    return state.entryMonotonicSet &&
      (this.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID) &&
      state.entryBootID.equals(this.header.tail_entry_boot_id) &&
      state.entryMonotonic !== this.header.tail_entry_monotonic;
  }

  parseData(offset, flags, size) {
    const payloadOffset = this.compact ? COMPACT_DATA_OBJECT_HEADER_SIZE : DATA_OBJECT_HEADER_SIZE;
    this.validateDataPayloadSize(offset, size, payloadOffset);
    const sizeNumber = u64ToNumber(size, `DATA size at offset ${offset}`);
    const storedPayload = this.data.subarray(offset + payloadOffset, offset + sizeNumber);
    const hashPayload = this.dataHashPayload(flags, storedPayload, offset);
    const storedHash = this.u64(offset + 16);
    const computedHash = this.hash(hashPayload);
    if (storedHash !== computedHash) {
      throw new ObjectGraphVerificationError(`DATA hash mismatch at offset ${offset}`);
    }
    const obj = this.readDataMetadata(offset, storedHash);
    this.validateDataMetadata(offset, obj);
    this.dataObjects.set(offset, obj);
  }

  validateDataPayloadSize(offset, size, payloadOffset) {
    if (size <= BigInt(payloadOffset)) {
      throw new ObjectGraphVerificationError(`DATA object at offset ${offset} has no payload`);
    }
  }

  dataHashPayload(flags, storedPayload, offset) {
    return flags ? this.decompressPayload(flags, storedPayload, offset) : storedPayload;
  }

  readDataMetadata(offset, storedHash) {
    return {
      hash: storedHash,
      next_hash_offset: this.u64n(offset + 24, 'DATA next_hash_offset'),
      next_field_offset: this.u64n(offset + 32, 'DATA next_field_offset'),
      entry_offset: this.u64n(offset + 40, 'DATA entry_offset'),
      entry_array_offset: this.u64n(offset + 48, 'DATA entry_array_offset'),
      n_entries: this.u64(offset + 56),
      tail_entry_array_offset: this.compact ? this.data.readUInt32LE(offset + 64) : 0,
      tail_entry_array_n_entries: this.compact ? this.data.readUInt32LE(offset + 68) : 0,
    };
  }

  validateDataMetadata(offset, obj) {
    if ((obj.entry_offset === 0) !== (obj.n_entries === 0n)) {
      throw new ObjectGraphVerificationError(`DATA object at offset ${offset} has bad n_entries`);
    }
    for (const field of ['next_hash_offset', 'next_field_offset', 'entry_offset', 'entry_array_offset']) {
      this.validOffset(obj[field], `DATA ${offset} ${field}`);
    }
    if (obj.n_entries < 2n && obj.entry_array_offset !== 0) {
      throw new ObjectGraphVerificationError(`DATA object at offset ${offset} has unexpected entry array`);
    }
    if (obj.n_entries >= 2n && obj.entry_array_offset === 0) {
      throw new ObjectGraphVerificationError(`DATA object at offset ${offset} is missing entry array`);
    }
  }

  parseField(offset, size) {
    if (size <= BigInt(FIELD_OBJECT_HEADER_SIZE)) {
      throw new ObjectGraphVerificationError(`FIELD object at offset ${offset} has no payload`);
    }
    const sizeNumber = u64ToNumber(size, `FIELD size at offset ${offset}`);
    const payload = this.data.subarray(offset + FIELD_OBJECT_HEADER_SIZE, offset + sizeNumber);
    const storedHash = this.u64(offset + 16);
    const computedHash = this.hash(payload);
    if (storedHash !== computedHash) throw new ObjectGraphVerificationError(`FIELD hash mismatch at offset ${offset}`);
    const obj = {
      hash: storedHash,
      next_hash_offset: this.u64n(offset + 24, 'FIELD next_hash_offset'),
      head_data_offset: this.u64n(offset + 32, 'FIELD head_data_offset'),
    };
    this.validOffset(obj.next_hash_offset, `FIELD ${offset} next_hash_offset`);
    this.validOffset(obj.head_data_offset, `FIELD ${offset} head_data_offset`);
    this.fieldObjects.set(offset, obj);
  }

  parseEntry(offset, size) {
    const itemSize = this.compact ? COMPACT_ENTRY_ITEM_SIZE : REGULAR_ENTRY_ITEM_SIZE;
    if (size < BigInt(ENTRY_OBJECT_HEADER_SIZE)) throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} is too small`);
    if ((size - BigInt(ENTRY_OBJECT_HEADER_SIZE)) % BigInt(itemSize) !== 0n) {
      throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} has unaligned items`);
    }
    const sizeNumber = u64ToNumber(size, `ENTRY size at offset ${offset}`);
    const itemOffsets = [];
    for (let itemOffset = offset + ENTRY_OBJECT_HEADER_SIZE; itemOffset < offset + sizeNumber; itemOffset += itemSize) {
      const item = this.compact ? this.data.readUInt32LE(itemOffset) : this.u64n(itemOffset, 'ENTRY item');
      if (item === 0) throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} has zero item`);
      this.validOffset(item, `ENTRY ${offset} item`);
      itemOffsets.push(item);
    }
    if (itemOffsets.length === 0) throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} has no items`);
    const entry = {
      seqnum: this.u64(offset + 16),
      realtime: this.u64(offset + 24),
      monotonic: this.u64(offset + 32),
      boot_id: Buffer.from(this.data.subarray(offset + 40, offset + 56)),
      items: itemOffsets,
    };
    if (entry.seqnum === 0n) throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} has zero seqnum`);
    if (entry.realtime === 0n) throw new ObjectGraphVerificationError(`ENTRY object at offset ${offset} has zero realtime`);
    this.entryObjects.set(offset, entry);
    return entry;
  }

  parseHashTable(offset, typ, size) {
    if (size < BigInt(OBJECT_HEADER_SIZE + HASH_ITEM_SIZE)) {
      throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} at offset ${offset} is too small`);
    }
    if ((size - BigInt(OBJECT_HEADER_SIZE)) % BigInt(HASH_ITEM_SIZE) !== 0n) {
      throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} at offset ${offset} has unaligned items`);
    }
    const tableOffset = typ === OBJECT_TYPE_DATA_HASH_TABLE
      ? this.header.data_hash_table_offset
      : this.header.field_hash_table_offset;
    const tableSize = typ === OBJECT_TYPE_DATA_HASH_TABLE
      ? this.header.data_hash_table_size
      : this.header.field_hash_table_size;
    if (tableOffset !== BigInt(offset + OBJECT_HEADER_SIZE)) {
      throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} header offset mismatch`);
    }
    if (tableSize !== size - BigInt(OBJECT_HEADER_SIZE)) {
      throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} header size mismatch`);
    }
    const sizeNumber = u64ToNumber(size, `hash table size at offset ${offset}`);
    for (let itemOffset = offset + OBJECT_HEADER_SIZE; itemOffset < offset + sizeNumber; itemOffset += HASH_ITEM_SIZE) {
      const head = this.u64n(itemOffset, 'hash bucket head');
      const tail = this.u64n(itemOffset + 8, 'hash bucket tail');
      if ((head === 0) !== (tail === 0)) {
        throw new ObjectGraphVerificationError(`${OBJECT_TYPES.get(typ)} bucket head/tail mismatch`);
      }
      this.validOffset(head, `${OBJECT_TYPES.get(typ)} bucket head`);
      this.validOffset(tail, `${OBJECT_TYPES.get(typ)} bucket tail`);
    }
  }

  parseEntryArray(offset, size) {
    const itemSize = this.compact ? COMPACT_OFFSET_ARRAY_ITEM_SIZE : REGULAR_OFFSET_ARRAY_ITEM_SIZE;
    if (size < BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE + itemSize)) {
      throw new ObjectGraphVerificationError(`ENTRY_ARRAY object at offset ${offset} is too small`);
    }
    if ((size - BigInt(OFFSET_ARRAY_OBJECT_HEADER_SIZE)) % BigInt(itemSize) !== 0n) {
      throw new ObjectGraphVerificationError(`ENTRY_ARRAY object at offset ${offset} has unaligned items`);
    }
    const sizeNumber = u64ToNumber(size, `ENTRY_ARRAY size at offset ${offset}`);
    const items = [];
    for (let itemOffset = offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE; itemOffset < offset + sizeNumber; itemOffset += itemSize) {
      const item = this.compact ? this.data.readUInt32LE(itemOffset) : this.u64n(itemOffset, 'ENTRY_ARRAY item');
      if (item !== 0) this.validOffset(item, `ENTRY_ARRAY ${offset} item`);
      items.push(item);
    }
    const next = this.u64n(offset + 16, 'ENTRY_ARRAY next');
    this.validOffset(next, `ENTRY_ARRAY ${offset} next`);
    this.entryArrays.set(offset, { next, items });
  }

  validateHeaderCounts() {
    const expected = new Map([
      ['n_objects', BigInt(this.order.length)],
      ['n_entries', this.count(OBJECT_TYPE_ENTRY)],
      ['n_data', this.count(OBJECT_TYPE_DATA)],
      ['n_fields', this.count(OBJECT_TYPE_FIELD)],
      ['n_tags', this.count(OBJECT_TYPE_TAG)],
      ['n_entry_arrays', this.count(OBJECT_TYPE_ENTRY_ARRAY)],
    ]);
    const fieldEnds = new Map([
      ['n_objects', 152],
      ['n_entries', 160],
      ['n_data', 216],
      ['n_fields', 224],
      ['n_tags', 232],
      ['n_entry_arrays', 240],
    ]);
    for (const [field, value] of expected) {
      if (this.headerHas(fieldEnds.get(field)) && this.header[field] !== value) {
        throw new ObjectGraphVerificationError(`header ${field} mismatch`);
      }
    }
  }

  validateMainEntryArrayPresence() {
    if (this.header.entry_array_offset !== 0n && !this.mainEntryArrayFound) {
      throw new ObjectGraphVerificationError('missing main entry array');
    }
    if (this.header.n_entries !== 0n && this.header.entry_array_offset === 0n) {
      throw new ObjectGraphVerificationError('entry_array_offset is zero with entries recorded');
    }
  }

  validateTailMetadata() {
    if (this.entryObjects.size === 0) {
      if (this.header.n_entries !== 0n) throw new ObjectGraphVerificationError('entries recorded but no ENTRY objects found');
      return;
    }
    const entries = Array.from(this.entryObjects.entries()).sort((a, b) => compareBigInt(a[1].seqnum, b[1].seqnum));
    const [headOffset, head] = entries[0];
    const [tailOffset, tail] = entries[entries.length - 1];
    this.validateHeadTailSeqnum(head, tail);
    this.validateHeadTailRealtime(head, tail);
    this.validateTailBootMetadata(tail);
    this.validateTailEntryOffset(tailOffset);
    if (headOffset === 0) throw new ObjectGraphVerificationError('head entry offset is zero');
  }

  validateHeadTailSeqnum(head, tail) {
    if (this.header.head_entry_seqnum !== head.seqnum) throw new ObjectGraphVerificationError('head_entry_seqnum mismatch');
    if (this.header.tail_entry_seqnum !== tail.seqnum) throw new ObjectGraphVerificationError('tail_entry_seqnum mismatch');
  }

  validateHeadTailRealtime(head, tail) {
    if (this.header.head_entry_realtime !== head.realtime) throw new ObjectGraphVerificationError('head_entry_realtime mismatch');
    if (this.header.tail_entry_realtime !== tail.realtime) throw new ObjectGraphVerificationError('tail_entry_realtime mismatch');
  }

  validateTailBootMetadata(tail) {
    if (!(this.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID)) return;
    if (this.header.tail_entry_monotonic !== tail.monotonic) throw new ObjectGraphVerificationError('tail_entry_monotonic mismatch');
    if (!this.header.tail_entry_boot_id.equals(tail.boot_id)) throw new ObjectGraphVerificationError('tail_entry_boot_id mismatch');
  }

  validateTailEntryOffset(tailOffset) {
    if (this.headerHas(272) && this.header.tail_entry_offset !== BigInt(tailOffset)) {
      throw new ObjectGraphVerificationError('tail_entry_offset mismatch');
    }
  }

  validateGlobalEntryArray() {
    const entries = this.walkEntryArrayChain(
      this.hnum('entry_array_offset'),
      this.hnum('n_entries'),
      'global entry array',
    );
    if (BigInt(entries.length) !== this.header.n_entries) {
      throw new ObjectGraphVerificationError('global entry array count mismatch');
    }
    let last = 0;
    entries.forEach((entryOffset, index) => {
      if (entryOffset <= last) throw new ObjectGraphVerificationError('global entry array is not sorted');
      if (!this.entryObjects.has(entryOffset)) throw new ObjectGraphVerificationError('global entry array references missing ENTRY');
      last = entryOffset;
      this.validateEntryDataLinks(entryOffset, index + 1 === entries.length);
    });
  }

  validateDataHashTable() {
    const tableOffset = this.hnum('data_hash_table_offset');
    const tableSize = this.hnum('data_hash_table_size');
    if (tableOffset === 0 || tableSize === 0) return;
    const bucketCount = Math.floor(tableSize / HASH_ITEM_SIZE);
    for (let bucketIndex = 0; bucketIndex < bucketCount; bucketIndex++) {
      const itemOffset = tableOffset + bucketIndex * HASH_ITEM_SIZE;
      let current = this.u64n(itemOffset, 'data hash bucket head');
      const tail = this.u64n(itemOffset + 8, 'data hash bucket tail');
      let last = 0;
      const seen = new Set();
      while (current !== 0) {
        if (seen.has(current)) throw new ObjectGraphVerificationError('data hash chain cycle');
        seen.add(current);
        const obj = this.dataObjects.get(current);
        if (!obj) throw new ObjectGraphVerificationError('data hash chain references missing DATA');
        if (obj.hash % BigInt(bucketCount) !== BigInt(bucketIndex)) {
          throw new ObjectGraphVerificationError('data hash bucket mismatch');
        }
        this.validateDataEntryArray(current, obj);
        if (obj.next_hash_offset !== 0 && obj.next_hash_offset <= current) {
          throw new ObjectGraphVerificationError('data hash chain points backwards');
        }
        last = current;
        current = obj.next_hash_offset;
      }
      if (last !== tail) throw new ObjectGraphVerificationError('data hash bucket tail mismatch');
    }
  }

  validateEntryDataLinks(entryOffset, lastEntry) {
    const entry = this.entryObjects.get(entryOffset);
    for (const dataOffset of entry.items) {
      const data = this.dataObjects.get(dataOffset);
      if (!data) throw new ObjectGraphVerificationError('entry references missing DATA object');
      if (!this.dataObjectInHashTable(dataOffset, data.hash)) {
        throw new ObjectGraphVerificationError('entry DATA object missing from hash table');
      }
      if (!this.dataReferencesEntry(data, entryOffset) && !lastEntry) {
        throw new ObjectGraphVerificationError('entry not referenced by linked DATA object');
      }
    }
  }

  validateDataEntryArray(dataOffset, data) {
    const nEntries = u64ToNumber(data.n_entries, `DATA ${dataOffset} n_entries`);
    if (nEntries === 0) return;
    if (!this.entryObjects.has(data.entry_offset)) throw new ObjectGraphVerificationError('DATA inline entry is missing');
    let last = data.entry_offset;
    if (data.entry_array_offset && nEntries < 2) {
      throw new ObjectGraphVerificationError('DATA entry array present with fewer than two entries');
    }
    for (const entryOffset of this.walkEntryArrayChain(data.entry_array_offset, nEntries - 1, `DATA ${dataOffset} entry array`)) {
      if (entryOffset <= last) throw new ObjectGraphVerificationError('DATA entry array is not sorted');
      last = entryOffset;
    }
  }

  walkEntryArrayChain(startOffset, usedCount, label) {
    if (usedCount === 0) {
      if (startOffset !== 0) throw new ObjectGraphVerificationError(`${label} has start offset with zero entries`);
      return [];
    }
    if (startOffset === 0) throw new ObjectGraphVerificationError(`${label} is missing`);
    const entries = [];
    let remaining = usedCount;
    let current = startOffset;
    const seen = new Set();
    while (remaining > 0) {
      const array = this.readEntryArrayChainNode(current, label, seen);
      const usedHere = this.appendEntryArrayChainItems(entries, array, remaining, label);
      remaining -= usedHere;
      if (remaining === 0) break;
      if (array.next === 0) throw new ObjectGraphVerificationError(`${label} ended early`);
      current = array.next;
    }
    return entries;
  }

  readEntryArrayChainNode(current, label, seen) {
    if (seen.has(current)) throw new ObjectGraphVerificationError(`${label} has a cycle`);
    seen.add(current);
    const array = this.entryArrays.get(current);
    if (!array) throw new ObjectGraphVerificationError(`${label} references missing ENTRY_ARRAY`);
    if (array.next !== 0 && array.next <= current) {
      throw new ObjectGraphVerificationError(`${label} next pointer is not increasing`);
    }
    return array;
  }

  appendEntryArrayChainItems(entries, array, remaining, label) {
    const usedHere = Math.min(remaining, array.items.length);
    for (let i = 0; i < usedHere; i++) {
      const item = array.items[i];
      if (item === 0) throw new ObjectGraphVerificationError(`${label} has zero used item`);
      if (!this.entryObjects.has(item)) throw new ObjectGraphVerificationError(`${label} references missing ENTRY`);
      entries.push(item);
    }
    return usedHere;
  }

  dataObjectInHashTable(dataOffset, dataHash) {
    const tableOffset = this.hnum('data_hash_table_offset');
    const tableSize = this.hnum('data_hash_table_size');
    if (tableOffset === 0 || tableSize === 0) return false;
    const bucketCount = Math.floor(tableSize / HASH_ITEM_SIZE);
    const bucket = Number(dataHash % BigInt(bucketCount));
    let current = this.u64n(tableOffset + bucket * HASH_ITEM_SIZE, 'data hash bucket head');
    const seen = new Set();
    while (current !== 0) {
      if (seen.has(current)) throw new ObjectGraphVerificationError('data hash chain cycle');
      seen.add(current);
      if (current === dataOffset) return true;
      const obj = this.dataObjects.get(current);
      if (!obj) throw new ObjectGraphVerificationError('data hash chain references missing DATA');
      current = obj.next_hash_offset;
    }
    return false;
  }

  dataReferencesEntry(data, entryOffset) {
    if (data.entry_offset === entryOffset) return true;
    const nEntries = u64ToNumber(data.n_entries, 'DATA n_entries');
    for (const item of this.walkEntryArrayChain(data.entry_array_offset, Math.max(0, nEntries - 1), 'DATA entry array lookup')) {
      if (item === entryOffset) return true;
    }
    return false;
  }

  validOffset(offset, label) {
    if (offset === 0) return;
    if (offset % 8 !== 0) throw new ObjectGraphVerificationError(`${label} offset ${offset} is not aligned`);
    if (offset < this.hnum('header_size') || offset > this.hnum('tail_object_offset')) {
      throw new ObjectGraphVerificationError(`${label} offset ${offset} outside object range`);
    }
  }

  hash(payload) {
    if (this.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH) {
      return sipHash24(this.header.file_id, payload);
    }
    return jenkinsHash64(payload);
  }

  decompressPayload(flags, payload, offset) {
    try {
      if (flags & OBJECT_COMPRESSED_ZSTD) return decompressZstdDataPayload(payload);
      if (flags & OBJECT_COMPRESSED_XZ) return decompressXzDataPayload(payload);
      if (flags & OBJECT_COMPRESSED_LZ4) return decompressLz4DataPayload(payload);
    } catch (err) {
      throw new ObjectGraphVerificationError(`DATA decompression failed at offset ${offset}: ${err.message}`);
    }
    return payload;
  }

  headerHas(end) {
    return this.header.header_size >= BigInt(end) && this.data.length >= end;
  }

  hnum(field) {
    return u64ToNumber(this.header[field], field);
  }

  u64(offset) {
    if (offset + 8 > this.data.length) throw new ObjectGraphVerificationError(`uint64 read at ${offset} exceeds file bounds`);
    return this.data.readBigUInt64LE(offset);
  }

  u64n(offset, context) {
    return u64ToNumber(this.u64(offset), context);
  }

  count(typ) {
    return this.counts.get(typ) || 0n;
  }
}

function align8(value) {
  return (value + 7n) & ~7n;
}

function flagCount(value) {
  let v = value;
  let count = 0;
  while (v) {
    count += v & 1;
    v >>= 1;
  }
  return count;
}

function u64ToNumber(value, context) {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new ObjectGraphVerificationError(`${context} exceeds JavaScript safe integer range`);
  }
  return Number(value);
}

function compareBigInt(a, b) {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}
