import { closeSync, fsyncSync, openSync, readSync } from 'node:fs';
import { dirname } from 'node:path';
import { readUint64LE } from './binary.js';
import {
  HEADER_SIZE,
  OBJECT_HEADER_SIZE,
  INCOMPATIBLE_KEYED_HASH,
  INCOMPATIBLE_COMPRESSED_XZ,
  INCOMPATIBLE_COMPRESSED_ZSTD,
  INCOMPATIBLE_COMPRESSED_LZ4,
  INCOMPATIBLE_COMPACT,
  parseFileHeader,
  parseObjectHeader,
} from './header.js';

const FIELD_CACHE_MAX_PAYLOAD_LEN = 128;

export function readAppendHeaderFromFd(fd) {
  const headerBuf = Buffer.alloc(HEADER_SIZE);
  const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
  if (bytesRead < HEADER_SIZE) throw new Error('cannot read journal header');
  return parseFileHeader(headerBuf);
}

export function validateAppendHeaderForWrite(header) {
  const supportedWriterIncompatible = INCOMPATIBLE_KEYED_HASH |
    INCOMPATIBLE_COMPRESSED_XZ |
    INCOMPATIBLE_COMPRESSED_ZSTD |
    INCOMPATIBLE_COMPRESSED_LZ4 |
    INCOMPATIBLE_COMPACT;
  if ((header.incompatible_flags & ~supportedWriterIncompatible) !== 0) {
    throw new Error('unsupported journal: incompatible flags');
  }
  if ((header.incompatible_flags & INCOMPATIBLE_KEYED_HASH) === 0) {
    throw new Error('unsupported journal: keyed hash required');
  }
  if (header.header_size < BigInt(HEADER_SIZE)) {
    throw new Error('unsupported journal: outdated header');
  }
  if (header.data_hash_table_offset === 0n || header.field_hash_table_offset === 0n || header.tail_object_offset === 0n) {
    throw new Error('invalid journal: missing hash tables');
  }
}

export function openedMonotonicBaseMs(header) {
  return header.tail_entry_monotonic > 0n ? Number(header.tail_entry_monotonic / 1000n) : 0;
}

export function readObjectSizeFromFd(fd, offset) {
  const buf = Buffer.alloc(8);
  readSync(fd, buf, 0, 8, Number(offset) + 8);
  return readUint64LE(buf, 0);
}

export function readObjectHeaderFromFd(fd, offset) {
  const buf = Buffer.alloc(OBJECT_HEADER_SIZE);
  readSync(fd, buf, 0, OBJECT_HEADER_SIZE, Number(offset));
  return parseObjectHeader(buf, 0);
}

export function fieldCacheKey(payload) {
  if (payload.length > FIELD_CACHE_MAX_PAYLOAD_LEN) return null;
  return payload.toString('base64');
}

export function syncParentDirectory(path) {
  if (process.platform === 'win32') return false;
  const dirFd = openSync(dirname(path), 'r');
  try {
    fsyncSync(dirFd);
    return true;
  } finally {
    closeSync(dirFd);
  }
}
