// Compression support and journal file name helpers.

import { zstdDecompressSync } from 'node:zlib';
import { safeReadFileSync } from './fs-safe.js';

export const MAX_UNCOMPRESSED_DATA_OBJECT_SIZE = 768 * 1024 * 1024;

// Decompress zstd bytes or file path to a Buffer.
export function decompressZstSync(input) {
  const src = Buffer.isBuffer(input) ? input : safeReadFileSync(input);
  return zstdDecompressSync(src);
}

export function decompressZstdDataPayload(payload) {
  const src = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  return zstdDecompressSync(src, { maxOutputLength: MAX_UNCOMPRESSED_DATA_OBJECT_SIZE });
}

// Check if a filename is a recognized journal file variant.
export function isJournalFileName(name) {
  return name.endsWith('.journal') ||
         name.endsWith('.journal~') ||
         name.endsWith('.journal.zst') ||
         name.endsWith('.journal~.zst');
}

// Check if a path ends with .zst.
export function isZstFile(path) {
  return path.endsWith('.zst');
}
