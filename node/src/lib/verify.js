// Journal file verification.
// Validates structural integrity of unsealed journal files.
// Sealed FSS tag/HMAC verification is implemented for sealed files with a key.

import { createHmac, timingSafeEqual } from 'node:crypto';
import {
  INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPRESSED_ZSTD, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS,
  HEADER_MIN_SIZE, OBJECT_HEADER_SIZE, OBJECT_TYPE_DATA,
  OBJECT_TYPE_FIELD, OBJECT_TYPE_ENTRY, OBJECT_TYPE_DATA_HASH_TABLE,
  OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_TAG,
  DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, FIELD_OBJECT_HEADER_SIZE,
  OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
} from './header.js';
import { fsprgGenMK, fsprgGenState0, fsprgSeek, fsprgGetKey, RECOMMENDED_SECPAR } from './fss.js';
import { TAG_LENGTH } from './seal.js';
import { ObjectGraphVerificationError, verifyObjectGraph } from './verify-graph.js';
import { openVerificationByteSource } from './verify-adapter.js';

const MAX_U64 = (1n << 64n) - 1n;

export class VerificationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'VerificationError';
  }
}

/**
 * Validate the structural integrity of a journal file.
 *
 * Opens the file (decompressing .zst if needed), validates the header,
 * and walks all entries and their referenced data objects strictly.
 * Any parse or decompression error is reported as a VerificationError.
 *
 * For sealed journals, this validates structure only; use verifyFileWithKey()
 * when TAG/HMAC verification is required.
 */
export function verifyFile(path, options = {}) {
  let reader = null;
  try {
    const opened = openVerificationByteSource(path, options);
    reader = opened.reader;
    verifyObjectGraph(opened.source);
    verifyReaderStrict(reader);
  } catch (err) {
    if (err instanceof ObjectGraphVerificationError) {
      throw new VerificationError(`journal verification failed: corrupt object graph: ${err.message}`);
    }
    throw new VerificationError(
      `journal verification failed: corrupt or unreadable file: ${err.message}`
    );
  } finally {
    reader?.close();
  }
}

/**
 * Validate the integrity of a journal file with an optional verification key.
 * For sealed files, parses the key and validates TAG/HMAC chains.
 * For unsealed files, behaves like verifyFile.
 */
export function verifyFileWithKey(path, verificationKey, options = {}) {
  let reader = null;
  try {
    const opened = openVerificationByteSource(path, options);
    reader = opened.reader;
    const { source } = opened;
    if (source.length < HEADER_MIN_SIZE) {
      throw new VerificationError('journal verification failed: file too small');
    }
    verifyObjectGraph(source);

    const header = reader.header;
    const sealed = (header.compatible_flags & COMPATIBLE_SEALED) !== 0;
    if (sealed) {
      const { seed, startEpoch, intervalUsec } = parseVerificationKey(verificationKey);
      verifySealed(source, header, seed, startEpoch, intervalUsec);
    }
    verifyReaderStrict(reader);
  } catch (err) {
    if (err instanceof ObjectGraphVerificationError) {
      throw new VerificationError(`journal verification failed: corrupt object graph: ${err.message}`);
    }
    if (err instanceof VerificationError) throw err;
    throw new VerificationError(
      `journal verification failed: corrupt or unreadable file: ${err.message}`
    );
  } finally {
    reader?.close();
  }
}

function parseVerificationKey(key) {
  if (typeof key !== 'string') {
    throw new VerificationError('invalid verification key: not a string');
  }
  const { seed, next } = parseVerificationSeed(key);
  if (next >= key.length || key.charCodeAt(next) !== 0x2f) {
    throw new VerificationError('invalid verification key: missing / separator');
  }
  const start = parseKeyU64Part(key, next + 1, 'start');
  if (start.next >= key.length || key.charCodeAt(start.next) !== 0x2d) {
    throw new VerificationError('invalid verification key: bad start hex');
  }
  const interval = parseKeyU64Part(key, start.next + 1, 'interval');
  if (interval.next !== key.length) {
    throw new VerificationError('invalid verification key: trailing data');
  }
  if (interval.value === 0n) {
    throw new VerificationError('invalid verification key: zero interval');
  }

  return { seed, startEpoch: start.value, intervalUsec: interval.value };
}

function parseVerificationSeed(key) {
  const seed = Buffer.alloc(12);
  let i = 0;
  for (let c = 0; c < seed.length; c++) {
    while (i < key.length && key.charCodeAt(i) === 0x2d) i++;
    seed.writeUInt8(parseSeedByte(key, i), c);
    i += 2;
  }
  return { seed, next: i };
}

function parseSeedByte(key, offset) {
  if (offset + 2 > key.length) {
    throw new VerificationError('invalid verification key: seed too short');
  }
  const pair = key.slice(offset, offset + 2);
  if (!/^[0-9a-fA-F]{2}$/.test(pair)) {
    throw new VerificationError('invalid verification key: bad seed hex');
  }
  return parseInt(pair, 16);
}

function parseKeyU64Part(key, start, label) {
  const result = consumeHex(key, start);
  if (!result.ok) {
    throw new VerificationError(`invalid verification key: bad ${label} hex`);
  }
  const value = BigInt(`0x${key.slice(start, result.next)}`);
  if (value < 0n || value > MAX_U64) {
    throw new VerificationError(`invalid verification key: bad ${label} hex`);
  }
  return { value, next: result.next };
}

function consumeHex(s, start) {
  let i = start;
  while (i < s.length && isHexCode(s.charCodeAt(i))) i++;
  return { next: i, ok: i > start };
}

function isHexCode(code) {
  return (code >= 0x30 && code <= 0x39) ||
    (code >= 0x41 && code <= 0x46) ||
    (code >= 0x61 && code <= 0x66);
}

function verifyReaderStrict(reader) {
  const compact = (reader.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;
  let entryMonotonic = 0n;
  let entryMonotonicSet = false;
  let entryBootID = Buffer.alloc(16);

  for (const offset of reader.entryOffsets) {
    let entry;
    try {
      entry = reader._readEntryObjectAt(offset);
    } catch (err) {
      throw new VerificationError(
        `journal verification failed: corrupt entry object at offset ${offset}: ${err.message}`
      );
    }

    if (entryMonotonicSet && entry.boot_id.equals(entryBootID) && entryMonotonic > entry.monotonic) {
      throw new VerificationError(
        `journal verification failed: entry monotonic out of sync (${entryMonotonic} > ${entry.monotonic})`
      );
    }
    entryMonotonic = entry.monotonic;
    entryBootID = entry.boot_id;
    entryMonotonicSet = true;

    for (const item of entry.items) {
      const dataOff = Number(item.offset);
      try {
        const payload = reader._readDataPayloadAt(dataOff, false);
        if (payload.indexOf(0x3d) < 0) throw new Error('DATA object missing field separator');
      } catch (err) {
        throw new VerificationError(
          `journal verification failed: corrupt data object at offset ${dataOff} ` +
          `for entry at offset ${offset}: ${err.message}`
        );
      }
    }
  }

  if (compact !== ((reader.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0)) {
    throw new VerificationError('journal verification failed: compact flag changed during verification');
  }
}

function align8(v) {
  return (v + 7n) & ~7n;
}

function verifySealed(data, header, seed, startEpoch, intervalUsec) {
  const context = createSealVerificationContext(data, header, seed);
  const state = createSealVerificationState(context.headerSize);

  let reachedTailObject = context.tailObjectOffset === 0n;
  while (!reachedTailObject) {
    const frame = readSealObjectFrame(data, context, state.offset);
    validateSealObjectFlags(frame, header);
    state.nObjects++;
    processSealedObject(data, header, context, state, frame, seed, startEpoch, intervalUsec);
    reachedTailObject = BigInt(state.offset) === context.tailObjectOffset;
    if (reachedTailObject) break;
    state.offset += frame.alignedSizeNumber;
  }

  validateSealCounts(state, header);
}

function createSealVerificationContext(data, header, seed) {
  const { msk, mpk } = fsprgGenMK(seed, RECOMMENDED_SECPAR);
  const headerSize = u64ToNumber(header.header_size, 'header_size');
  if (headerSize < HEADER_MIN_SIZE || headerSize > data.length) {
    throw new VerificationError(`invalid header_size ${header.header_size}`);
  }
  return {
    msk,
    state0: fsprgGenState0(mpk, seed),
    isCompact: (header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0,
    headerSize,
    tailObjectOffset: header.tail_object_offset,
    fileSize: data.length,
  };
}

function createSealVerificationState(headerSize) {
  return {
    offset: headerSize,
    nObjects: 0n,
    nEntries: 0n,
    nTags: 0n,
    lastTagEnd: 0,
    lastEpoch: 0n,
    lastTagRealtime: 0n,
    entrySeqnum: 0n,
    entrySeqnumSet: false,
    entryMonotonic: 0n,
    entryMonotonicSet: false,
    entryBootID: Buffer.alloc(16),
    entryRealtime: 0n,
    entryRealtimeSet: false,
    maxEntryRealtime: 0n,
    minEntryRealtime: null,
  };
}

function readSealObjectFrame(data, context, offset) {
  if (BigInt(offset) > context.tailObjectOffset) {
    throw new VerificationError(`object offset ${offset} exceeds tail_object_offset ${context.tailObjectOffset}`);
  }
  if (offset + OBJECT_HEADER_SIZE > context.fileSize) {
    throw new VerificationError(`object header at offset ${offset} exceeds file bounds`);
  }
  const size = data.u64(offset + 8);
  const alignedSize = align8(size);
  if (size < BigInt(OBJECT_HEADER_SIZE)) {
    throw new VerificationError(`object size ${size} too small at offset ${offset}`);
  }
  if (BigInt(offset) + alignedSize > BigInt(context.fileSize)) {
    throw new VerificationError(`object at offset ${offset} with aligned size ${alignedSize} exceeds file bounds`);
  }
  return {
    offset,
    typ: data.u8(offset),
    flags: data.u8(offset + 1),
    size,
    alignedSizeNumber: u64ToNumber(alignedSize, `aligned object size at offset ${offset}`),
  };
}

function validateSealObjectFlags(frame, header) {
  if (sealCompressionFlagCount(frame.flags) > 1) {
    throw new VerificationError(`multiple compression flags at offset ${frame.offset}`);
  }
  if ((frame.flags & OBJECT_COMPRESSED_XZ) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ)) {
    throw new VerificationError(`XZ object in file without XZ support at offset ${frame.offset}`);
  }
  if ((frame.flags & OBJECT_COMPRESSED_LZ4) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4)) {
    throw new VerificationError(`LZ4 object in file without LZ4 support at offset ${frame.offset}`);
  }
  if ((frame.flags & OBJECT_COMPRESSED_ZSTD) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD)) {
    throw new VerificationError(`ZSTD object in file without ZSTD support at offset ${frame.offset}`);
  }
  if (frame.flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)) {
    throw new VerificationError(`unknown object flags 0x${frame.flags.toString(16)} at offset ${frame.offset}`);
  }
  if (frame.typ !== OBJECT_TYPE_DATA && frame.flags !== 0) {
    throw new VerificationError(`object type ${frame.typ} at offset ${frame.offset} has compression flags`);
  }
}

function processSealedObject(data, header, context, state, frame, seed, startEpoch, intervalUsec) {
  switch (frame.typ) {
    case OBJECT_TYPE_DATA:
    case OBJECT_TYPE_FIELD:
    case OBJECT_TYPE_DATA_HASH_TABLE:
    case OBJECT_TYPE_FIELD_HASH_TABLE:
    case OBJECT_TYPE_ENTRY_ARRAY:
      return;
    case OBJECT_TYPE_ENTRY:
      validateSealedEntry(data, header, state, frame);
      return;
    case OBJECT_TYPE_TAG:
      validateSealedTag(data, header, context, state, frame, seed, startEpoch, intervalUsec);
      return;
    default:
      throw new VerificationError(`unknown object type ${frame.typ} at offset ${frame.offset}`);
  }
}

function validateSealedEntry(data, header, state, frame) {
  if (state.nTags === 0n) {
    throw new VerificationError(`first entry before first tag at offset ${frame.offset}`);
  }
  const entry = readSealEntry(data, frame.offset);
  if (state.entryRealtimeSet && entry.realtime < state.lastTagRealtime) {
    throw new VerificationError(`older entry after newer tag at offset ${frame.offset}`);
  }
  validateSealedEntrySeqnum(entry.seqnum, header, state, frame.offset);
  validateSealedEntryMonotonic(entry, state, frame.offset);
  if (!state.entryRealtimeSet && entry.realtime !== header.head_entry_realtime) {
    throw new VerificationError(`head entry realtime mismatch at offset ${frame.offset}`);
  }
  state.entryRealtime = entry.realtime;
  state.entryRealtimeSet = true;
  if (entry.realtime > state.maxEntryRealtime) state.maxEntryRealtime = entry.realtime;
  if (state.minEntryRealtime === null || entry.realtime < state.minEntryRealtime) state.minEntryRealtime = entry.realtime;
  state.nEntries++;
}

function readSealEntry(data, offset) {
  return {
    seqnum: data.u64(offset + 16),
    realtime: data.u64(offset + 24),
    monotonic: data.u64(offset + 32),
    bootID: data.bytes(offset + 40, 16),
  };
}

function validateSealedEntrySeqnum(seqnum, header, state, offset) {
  if (!state.entrySeqnumSet && seqnum !== header.head_entry_seqnum) {
    throw new VerificationError(`head entry seqnum mismatch at offset ${offset}`);
  }
  if (state.entrySeqnumSet && state.entrySeqnum >= seqnum) {
    throw new VerificationError(`entry seqnum out of sync at offset ${offset}`);
  }
  state.entrySeqnum = seqnum;
  state.entrySeqnumSet = true;
}

function validateSealedEntryMonotonic(entry, state, offset) {
  if (state.entryMonotonicSet && entry.bootID.equals(state.entryBootID) && state.entryMonotonic > entry.monotonic) {
    throw new VerificationError(`entry monotonic out of sync at offset ${offset}`);
  }
  state.entryMonotonic = entry.monotonic;
  state.entryBootID = entry.bootID;
  state.entryMonotonicSet = true;
}

function validateSealedTag(data, header, context, state, frame, seed, startEpoch, intervalUsec) {
  if (frame.size !== BigInt(OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH)) {
    throw new VerificationError(`invalid tag object size ${frame.size} at offset ${frame.offset}`);
  }
  const seqnum = data.u64(frame.offset + 16);
  const epoch = data.u64(frame.offset + 24);
  if (seqnum !== state.nTags + 1n) {
    throw new VerificationError(`tag seqnum mismatch: got ${seqnum}, want ${state.nTags + 1n} at offset ${frame.offset}`);
  }
  validateSealedTagEpoch(epoch, header, state, frame.offset);
  const { rt, rtEnd } = tagRealtimeRange(startEpoch, epoch, intervalUsec);
  validateSealedTagRealtimeWindow(state, frame.offset, rt, rtEnd);
  verifyTagHmac(data, context, state, frame, seed, epoch);
  state.nTags++;
  state.lastTagEnd = frame.offset + frame.alignedSizeNumber;
  state.lastEpoch = epoch;
  state.lastTagRealtime = rt;
  state.minEntryRealtime = null;
}

function validateSealedTagEpoch(epoch, header, state, offset) {
  const sealedContinuous = (header.compatible_flags & COMPATIBLE_SEALED_CONTINUOUS) !== 0;
  if (sealedContinuous) {
    const ok = state.nTags === 0n || (state.nTags === 1n && epoch === state.lastEpoch) || epoch === state.lastEpoch + 1n;
    if (!ok) throw new VerificationError(`epoch not continuous: got ${epoch}, last ${state.lastEpoch} at offset ${offset}`);
    return;
  }
  if (epoch < state.lastEpoch) {
    throw new VerificationError(`epoch out of sync: got ${epoch}, last ${state.lastEpoch} at offset ${offset}`);
  }
}

function validateSealedTagRealtimeWindow(state, offset, rt, rtEnd) {
  if (state.entryRealtimeSet && state.entryRealtime >= rtEnd) {
    throw new VerificationError(`entry realtime ${state.entryRealtime} too late for tag end ${rtEnd} at offset ${offset}`);
  }
  if (state.maxEntryRealtime >= rtEnd) {
    throw new VerificationError(`max entry realtime ${state.maxEntryRealtime} too late for tag end ${rtEnd} at offset ${offset}`);
  }
  if (state.minEntryRealtime !== null && state.minEntryRealtime < rt) {
    throw new VerificationError(`entry realtime ${state.minEntryRealtime} too early for tag start ${rt} at offset ${offset}`);
  }
}

function verifyTagHmac(data, context, state, frame, seed, epoch) {
  const hm = createTagHmac(data, context, state, seed, epoch);
  hmacSealedObjectRange(hm, data, context, state, frame.offset);
  const computed = hm.digest();
  const stored = data.bytes(frame.offset + 32, TAG_LENGTH);
  if (stored.length !== TAG_LENGTH || !timingSafeEqual(computed, stored)) {
    throw new VerificationError(`tag failed verification at offset ${frame.offset}`);
  }
}

function createTagHmac(data, context, state, seed, epoch) {
  const fssState = fsprgSeek(context.state0, epoch, context.msk, seed);
  const key = fsprgGetKey(fssState, TAG_LENGTH, 0);
  const hm = createHmac('sha256', key);
  if (state.nTags === 0n) {
    data.updateHmac(hm, 0, 16);
    data.updateHmac(hm, 24, 32);
    data.updateHmac(hm, 72, 24);
    data.updateHmac(hm, 104, 32);
  }
  return hm;
}

function hmacSealedObjectRange(hm, data, context, state, tagOffset) {
  let q = state.nTags === 0n ? context.headerSize : state.lastTagEnd;
  while (q <= tagOffset) {
    const frame = readHmacObjectFrame(data, context, q);
    hmacObject(hm, data, q, frame.typ, frame.size, context.isCompact);
    q += frame.alignedSizeNumber;
  }
}

function readHmacObjectFrame(data, context, offset) {
  if (offset + OBJECT_HEADER_SIZE > context.fileSize) {
    throw new VerificationError(`object header at offset ${offset} exceeds file bounds`);
  }
  const size = data.u64(offset + 8);
  if (size < BigInt(OBJECT_HEADER_SIZE)) {
    throw new VerificationError(`HMAC object size ${size} too small at offset ${offset}`);
  }
  const alignedSize = align8(size);
  if (alignedSize < size || alignedSize === 0n) {
    throw new VerificationError(`HMAC object size ${size} overflows alignment at offset ${offset}`);
  }
  if (BigInt(offset) + alignedSize > BigInt(context.fileSize)) {
    throw new VerificationError(`HMAC object at offset ${offset} with aligned size ${alignedSize} exceeds file bounds`);
  }
  return {
    typ: data.u8(offset),
    size,
    alignedSizeNumber: u64ToNumber(alignedSize, `aligned object size at offset ${offset}`),
  };
}

function sealCompressionFlagCount(flags) {
  let count = 0;
  if (flags & OBJECT_COMPRESSED_XZ) count++;
  if (flags & OBJECT_COMPRESSED_LZ4) count++;
  if (flags & OBJECT_COMPRESSED_ZSTD) count++;
  return count;
}

function validateSealCounts(state, header) {
  if (state.nObjects !== header.n_objects) {
    throw new VerificationError(`object count mismatch: got ${state.nObjects}, want ${header.n_objects}`);
  }
  if (state.nEntries !== header.n_entries) {
    throw new VerificationError(`entry count mismatch: got ${state.nEntries}, want ${header.n_entries}`);
  }
  if (state.nTags !== header.n_tags) {
    throw new VerificationError(`tag count mismatch: got ${state.nTags}, want ${header.n_tags}`);
  }
}

function u64ToNumber(value, context) {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new VerificationError(`${context} exceeds JavaScript safe integer range`);
  }
  return Number(value);
}

function tagRealtimeRange(startEpoch, epoch, intervalUsec) {
  const absoluteEpoch = startEpoch + epoch;
  if (absoluteEpoch > MAX_U64) {
    throw new VerificationError('tag realtime overflow');
  }
  const rt = absoluteEpoch * intervalUsec;
  if (rt > MAX_U64) {
    throw new VerificationError('tag realtime overflow');
  }
  const rtEnd = rt + intervalUsec;
  if (rtEnd > MAX_U64) {
    throw new VerificationError('tag realtime overflow');
  }
  return { rt, rtEnd };
}

function hmacObject(hm, data, offset, typ, size, isCompact) {
  const sizeNumber = u64ToNumber(size, `object size at offset ${offset}`);
  data.updateHmac(hm, offset, OBJECT_HEADER_SIZE);

  switch (typ) {
    case OBJECT_TYPE_DATA:
      data.updateHmac(hm, offset + 16, 8);
      {
        let payloadOffset = DATA_OBJECT_HEADER_SIZE;
        if (isCompact) payloadOffset = COMPACT_DATA_OBJECT_HEADER_SIZE;
        if (size > BigInt(payloadOffset)) {
          data.updateHmac(hm, offset + payloadOffset, sizeNumber - payloadOffset);
        }
      }
      break;
    case OBJECT_TYPE_FIELD:
      data.updateHmac(hm, offset + 16, 8);
      if (size > BigInt(FIELD_OBJECT_HEADER_SIZE)) {
        data.updateHmac(hm, offset + FIELD_OBJECT_HEADER_SIZE, sizeNumber - FIELD_OBJECT_HEADER_SIZE);
      }
      break;
    case OBJECT_TYPE_ENTRY:
      if (size > BigInt(OBJECT_HEADER_SIZE)) {
        data.updateHmac(hm, offset + OBJECT_HEADER_SIZE, sizeNumber - OBJECT_HEADER_SIZE);
      }
      break;
    case OBJECT_TYPE_DATA_HASH_TABLE:
    case OBJECT_TYPE_FIELD_HASH_TABLE:
    case OBJECT_TYPE_ENTRY_ARRAY:
      // nothing beyond header
      break;
    case OBJECT_TYPE_TAG:
      data.updateHmac(hm, offset + OBJECT_HEADER_SIZE, 16);
      break;
  }
}
