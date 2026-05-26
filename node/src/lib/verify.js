// Journal file verification.
// Validates structural integrity of unsealed journal files.
// Sealed FSS tag/HMAC verification is implemented for sealed files with a key.

import { readFileSync, unlinkSync, rmdirSync } from 'node:fs';
import { createHmac, timingSafeEqual } from 'node:crypto';
import { dirname } from 'node:path';
import { FileReader } from './reader.js';
import { parseEntryObject, parseDataObject } from './entry.js';
import {
  INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPRESSED_ZSTD, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS,
  HEADER_MIN_SIZE, parseFileHeader, OBJECT_HEADER_SIZE, OBJECT_TYPE_DATA,
  OBJECT_TYPE_FIELD, OBJECT_TYPE_ENTRY, OBJECT_TYPE_DATA_HASH_TABLE,
  OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_TAG,
  DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, FIELD_OBJECT_HEADER_SIZE,
  OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
} from './header.js';
import { isZstFile, decompressZstToTemp } from './compress.js';
import { fsprgGenMK, fsprgGenState0, fsprgSeek, fsprgGetKey, RECOMMENDED_SECPAR } from './fss.js';
import { TAG_LENGTH } from './seal.js';

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
export function verifyFile(path) {
  let r;
  try {
    r = FileReader.open(path);
  } catch (err) {
    throw new VerificationError(
      `journal verification failed: corrupt or unreadable file: ${err.message}`
    );
  }

  try {
    // Verification walks internal parser state so corrupt data objects fail
    // instead of being skipped by the normal reader tolerance path.
    const buf = r.buffer;
    const compact = (r.header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;
    let entryMonotonic = 0n;
    let entryMonotonicSet = false;
    let entryBootID = Buffer.alloc(16);

    for (const offset of r.entryOffsets) {
      // Parse entry object strictly
      let e;
      try {
        e = parseEntryObject(buf, Number(offset), compact);
      } catch (err) {
        throw new VerificationError(
          `journal verification failed: corrupt entry object at offset ${offset}: ${err.message}`
        );
      }

      if (entryMonotonicSet && e.boot_id.equals(entryBootID) && entryMonotonic > e.monotonic) {
        throw new VerificationError(
          `journal verification failed: entry monotonic out of sync (${entryMonotonic} > ${e.monotonic})`
        );
      }
      entryMonotonic = e.monotonic;
      entryBootID = e.boot_id;
      entryMonotonicSet = true;

      // Parse each referenced data object strictly
      for (const item of e.items) {
        const dataOff = Number(item.offset);
        try {
          parseDataObject(buf, dataOff, compact);
        } catch (err) {
          throw new VerificationError(
            `journal verification failed: corrupt data object at offset ${dataOff} ` +
            `for entry at offset ${offset}: ${err.message}`
          );
        }
      }
    }
  } finally {
    r.close();
  }
}

/**
 * Validate the integrity of a journal file with an optional verification key.
 * For sealed files, parses the key and validates TAG/HMAC chains.
 * For unsealed files, behaves like verifyFile.
 */
export function verifyFileWithKey(path, verificationKey) {
  let data;
  let cleanupPath = null;
  try {
    if (isZstFile(path)) {
      cleanupPath = decompressZstToTemp(path, 'node-sdk-verify');
      data = readFileSync(cleanupPath);
    } else {
      data = readFileSync(path);
    }
  } catch (err) {
    throw new VerificationError(
      `journal verification failed: corrupt or unreadable file: ${err.message}`
    );
  } finally {
    if (cleanupPath) {
      try { unlinkSync(cleanupPath); } catch {}
      try { rmdirSync(dirname(cleanupPath)); } catch {}
    }
  }

  if (data.length < HEADER_MIN_SIZE) {
    throw new VerificationError('journal verification failed: file too small');
  }

  const header = parseFileHeader(data);
  const sealed = (header.compatible_flags & COMPATIBLE_SEALED) !== 0;

  if (!sealed) {
    return verifyFile(path);
  }

  const { seed, startEpoch, intervalUsec } = parseVerificationKey(verificationKey);
  verifySealed(data, header, seed, startEpoch, intervalUsec);
  return verifyFile(path);
}

function parseVerificationKey(key) {
  if (typeof key !== 'string') {
    throw new VerificationError('invalid verification key: not a string');
  }
  const seed = Buffer.alloc(12);
  let i = 0;
  for (let c = 0; c < 12; c++) {
    while (i < key.length && key[i] === '-') i++;
    if (i + 2 > key.length) {
      throw new VerificationError('invalid verification key: seed too short');
    }
    const pair = key.slice(i, i + 2);
    if (!/^[0-9a-fA-F]{2}$/.test(pair)) {
      throw new VerificationError('invalid verification key: bad seed hex');
    }
    const b = parseInt(pair, 16);
    seed[c] = b;
    i += 2;
  }
  if (i >= key.length || key[i] !== '/') {
    throw new VerificationError('invalid verification key: missing / separator');
  }
  i++;

  const startResult = consumeHex(key, i);
  if (!startResult.ok || startResult.next >= key.length || key[startResult.next] !== '-') {
    throw new VerificationError('invalid verification key: bad start hex');
  }
  const startEpoch = BigInt(`0x${key.slice(i, startResult.next)}`);
  if (startEpoch < 0n || startEpoch > MAX_U64) {
    throw new VerificationError('invalid verification key: bad start hex');
  }

  i = startResult.next + 1;
  const intervalResult = consumeHex(key, i);
  if (!intervalResult.ok) {
    throw new VerificationError('invalid verification key: bad interval hex');
  }
  const intervalUsec = BigInt(`0x${key.slice(i, intervalResult.next)}`);
  if (intervalResult.next !== key.length) {
    throw new VerificationError('invalid verification key: trailing data');
  }
  if (intervalUsec === 0n) {
    throw new VerificationError('invalid verification key: zero interval');
  }
  if (intervalUsec < 0n || intervalUsec > MAX_U64) {
    throw new VerificationError('invalid verification key: bad interval hex');
  }

  return { seed, startEpoch, intervalUsec };
}

function consumeHex(s, start) {
  let i = start;
  while (i < s.length && isHex(s[i])) i++;
  return { next: i, ok: i > start };
}

function isHex(ch) {
  return /^[0-9a-fA-F]$/.test(ch);
}

function align8(v) {
  return (v + 7n) & ~7n;
}

function verifySealed(data, header, seed, startEpoch, intervalUsec) {
  const isCompact = (header.incompatible_flags & INCOMPATIBLE_COMPACT) !== 0;

  const { msk, mpk } = fsprgGenMK(seed, RECOMMENDED_SECPAR);
  const state0 = fsprgGenState0(mpk, seed);

  const headerSize = u64ToNumber(header.header_size, 'header_size');
  const tailObjectOffset = header.tail_object_offset;
  const fileSize = data.length;
  if (headerSize < HEADER_MIN_SIZE || headerSize > fileSize) {
    throw new VerificationError(`invalid header_size ${header.header_size}`);
  }

  let nObjects = 0n;
  let nEntries = 0n;
  let nTags = 0n;
  let lastTagEnd = 0;
  let lastEpoch = 0n;
  let lastTagRealtime = 0n;
  let entrySeqnum = 0n;
  let entrySeqnumSet = false;
  let entryMonotonic = 0n;
  let entryMonotonicSet = false;
  let entryBootID = Buffer.alloc(16);
  let entryRealtime = 0n;
  let entryRealtimeSet = false;
  let maxEntryRealtime = 0n;
  let minEntryRealtime = null;

  let p = headerSize;
  while (true) {
    if (tailObjectOffset === 0n) break;
    if (BigInt(p) > tailObjectOffset) {
      throw new VerificationError(`object offset ${p} exceeds tail_object_offset ${tailObjectOffset}`);
    }
    if (p + OBJECT_HEADER_SIZE > fileSize) {
      throw new VerificationError(`object header at offset ${p} exceeds file bounds`);
    }

    const typ = data[p];
    const flags = data[p + 1];
    const size = data.readBigUInt64LE(p + 8);
    const alignedSize = align8(size);

    if (size < BigInt(OBJECT_HEADER_SIZE)) {
      throw new VerificationError(`object size ${size} too small at offset ${p}`);
    }
    if (BigInt(p) + alignedSize > BigInt(fileSize)) {
      throw new VerificationError(`object at offset ${p} with aligned size ${alignedSize} exceeds file bounds`);
    }
    const alignedSizeNumber = u64ToNumber(alignedSize, `aligned object size at offset ${p}`);

    let compressionFlags = 0;
    if (flags & OBJECT_COMPRESSED_XZ) compressionFlags++;
    if (flags & OBJECT_COMPRESSED_LZ4) compressionFlags++;
    if (flags & OBJECT_COMPRESSED_ZSTD) compressionFlags++;
    if (compressionFlags > 1) {
      throw new VerificationError(`multiple compression flags at offset ${p}`);
    }
    if ((flags & OBJECT_COMPRESSED_XZ) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ)) {
      throw new VerificationError(`XZ object in file without XZ support at offset ${p}`);
    }
    if ((flags & OBJECT_COMPRESSED_LZ4) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4)) {
      throw new VerificationError(`LZ4 object in file without LZ4 support at offset ${p}`);
    }
    if ((flags & OBJECT_COMPRESSED_ZSTD) && !(header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD)) {
      throw new VerificationError(`ZSTD object in file without ZSTD support at offset ${p}`);
    }
    if (flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)) {
      throw new VerificationError(`unknown object flags 0x${flags.toString(16)} at offset ${p}`);
    }

    nObjects++;

    switch (typ) {
      case OBJECT_TYPE_DATA:
        break;
      case OBJECT_TYPE_FIELD:
        break;
      case OBJECT_TYPE_ENTRY:
        if (nTags === 0n) {
          throw new VerificationError(`first entry before first tag at offset ${p}`);
        }
        {
          const eSeqnum = data.readBigUInt64LE(p + 16);
          const eRealtime = data.readBigUInt64LE(p + 24);
          const eMonotonic = data.readBigUInt64LE(p + 32);
          const eBootID = data.slice(p + 40, p + 56);

          if (entryRealtimeSet && eRealtime < lastTagRealtime) {
            throw new VerificationError(`older entry after newer tag at offset ${p}`);
          }
          if (!entrySeqnumSet) {
            if (eSeqnum !== header.head_entry_seqnum) {
              throw new VerificationError(`head entry seqnum mismatch at offset ${p}`);
            }
          } else {
            if (entrySeqnum >= eSeqnum) {
              throw new VerificationError(`entry seqnum out of sync at offset ${p}`);
            }
          }
          entrySeqnum = eSeqnum;
          entrySeqnumSet = true;

          if (entryMonotonicSet && eBootID.equals(entryBootID) && entryMonotonic > eMonotonic) {
            throw new VerificationError(`entry monotonic out of sync at offset ${p}`);
          }
          entryMonotonic = eMonotonic;
          entryBootID = eBootID;
          entryMonotonicSet = true;

          if (!entryRealtimeSet) {
            if (eRealtime !== header.head_entry_realtime) {
              throw new VerificationError(`head entry realtime mismatch at offset ${p}`);
            }
          }
          entryRealtime = eRealtime;
          entryRealtimeSet = true;

          if (eRealtime > maxEntryRealtime) maxEntryRealtime = eRealtime;
          if (minEntryRealtime === null || eRealtime < minEntryRealtime) minEntryRealtime = eRealtime;

          nEntries++;
        }
        break;
      case OBJECT_TYPE_DATA_HASH_TABLE:
        break;
      case OBJECT_TYPE_FIELD_HASH_TABLE:
        break;
      case OBJECT_TYPE_ENTRY_ARRAY:
        break;
      case OBJECT_TYPE_TAG:
        if (size !== BigInt(OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH)) {
          throw new VerificationError(`invalid tag object size ${size} at offset ${p}`);
        }
        {
          const seqnum = data.readBigUInt64LE(p + 16);
          const epoch = data.readBigUInt64LE(p + 24);

          if (seqnum !== nTags + 1n) {
            throw new VerificationError(`tag seqnum mismatch: got ${seqnum}, want ${nTags + 1n} at offset ${p}`);
          }

          const sealedContinuous = (header.compatible_flags & COMPATIBLE_SEALED_CONTINUOUS) !== 0;
          if (sealedContinuous) {
            const ok = nTags === 0n || (nTags === 1n && epoch === lastEpoch) || epoch === lastEpoch + 1n;
            if (!ok) {
              throw new VerificationError(`epoch not continuous: got ${epoch}, last ${lastEpoch} at offset ${p}`);
            }
          } else {
            if (epoch < lastEpoch) {
              throw new VerificationError(`epoch out of sync: got ${epoch}, last ${lastEpoch} at offset ${p}`);
            }
          }

          const { rt, rtEnd } = tagRealtimeRange(startEpoch, epoch, intervalUsec);

          if (entryRealtimeSet && entryRealtime >= rtEnd) {
            throw new VerificationError(`entry realtime ${entryRealtime} too late for tag end ${rtEnd} at offset ${p}`);
          }
          if (maxEntryRealtime >= rtEnd) {
            throw new VerificationError(`max entry realtime ${maxEntryRealtime} too late for tag end ${rtEnd} at offset ${p}`);
          }
          if (minEntryRealtime !== null && minEntryRealtime < rt) {
            throw new VerificationError(`entry realtime ${minEntryRealtime} too early for tag start ${rt} at offset ${p}`);
          }

          // Compute HMAC
          const state = fsprgSeek(state0, epoch, msk, seed);
          const key = fsprgGetKey(state, TAG_LENGTH, 0);
          const hm = createHmac('sha256', key);

          if (nTags === 0n) {
            hm.update(data.slice(0, 16));
            hm.update(data.slice(24, 56));
            hm.update(data.slice(72, 96));
            hm.update(data.slice(104, 136));
          }

          let q = lastTagEnd;
          if (nTags === 0n) {
            q = headerSize;
          }

          while (q <= p) {
            if (q + OBJECT_HEADER_SIZE > fileSize) {
              throw new VerificationError(`object header at offset ${q} exceeds file bounds`);
            }
            const qTyp = data[q];
            const qSize = data.readBigUInt64LE(q + 8);
            if (qSize < BigInt(OBJECT_HEADER_SIZE)) {
              throw new VerificationError(`HMAC object size ${qSize} too small at offset ${q}`);
            }
            const qAlignedSize = align8(qSize);
            if (qAlignedSize < qSize || qAlignedSize === 0n) {
              throw new VerificationError(`HMAC object size ${qSize} overflows alignment at offset ${q}`);
            }
            if (BigInt(q) + qAlignedSize > BigInt(fileSize)) {
              throw new VerificationError(`HMAC object at offset ${q} with aligned size ${qAlignedSize} exceeds file bounds`);
            }
            const qAlignedSizeNumber = u64ToNumber(qAlignedSize, `aligned object size at offset ${q}`);
            hmacObject(hm, data, q, qTyp, qSize, isCompact);
            q += qAlignedSizeNumber;
          }

          const computed = hm.digest();
          const stored = data.slice(p + 32, p + 32 + TAG_LENGTH);
          if (stored.length !== TAG_LENGTH || !timingSafeEqual(computed, stored)) {
            throw new VerificationError(`tag failed verification at offset ${p}`);
          }

          nTags++;
          lastTagEnd = p + alignedSizeNumber;
          lastEpoch = epoch;
          lastTagRealtime = rt;
          minEntryRealtime = null;
        }
        break;
      default:
        throw new VerificationError(`unknown object type ${typ} at offset ${p}`);
    }

    if (BigInt(p) === tailObjectOffset) break;
    p += alignedSizeNumber;
  }

  if (nObjects !== header.n_objects) {
    throw new VerificationError(`object count mismatch: got ${nObjects}, want ${header.n_objects}`);
  }
  if (nEntries !== header.n_entries) {
    throw new VerificationError(`entry count mismatch: got ${nEntries}, want ${header.n_entries}`);
  }
  if (nTags !== header.n_tags) {
    throw new VerificationError(`tag count mismatch: got ${nTags}, want ${header.n_tags}`);
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
  hm.update(data.slice(offset, offset + OBJECT_HEADER_SIZE));

  switch (typ) {
    case OBJECT_TYPE_DATA:
      hm.update(data.slice(offset + 16, offset + 24));
      {
        let payloadOffset = DATA_OBJECT_HEADER_SIZE;
        if (isCompact) payloadOffset = COMPACT_DATA_OBJECT_HEADER_SIZE;
        if (size > BigInt(payloadOffset)) {
          hm.update(data.slice(offset + payloadOffset, offset + sizeNumber));
        }
      }
      break;
    case OBJECT_TYPE_FIELD:
      hm.update(data.slice(offset + 16, offset + 24));
      if (size > BigInt(FIELD_OBJECT_HEADER_SIZE)) {
        hm.update(data.slice(offset + FIELD_OBJECT_HEADER_SIZE, offset + sizeNumber));
      }
      break;
    case OBJECT_TYPE_ENTRY:
      if (size > BigInt(OBJECT_HEADER_SIZE)) {
        hm.update(data.slice(offset + OBJECT_HEADER_SIZE, offset + sizeNumber));
      }
      break;
    case OBJECT_TYPE_DATA_HASH_TABLE:
    case OBJECT_TYPE_FIELD_HASH_TABLE:
    case OBJECT_TYPE_ENTRY_ARRAY:
      // nothing beyond header
      break;
    case OBJECT_TYPE_TAG:
      hm.update(data.slice(offset + OBJECT_HEADER_SIZE, offset + OBJECT_HEADER_SIZE + 16));
      break;
  }
}
