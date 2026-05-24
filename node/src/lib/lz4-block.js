import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const lz4 = require('lz4js');

export const MAX_UNCOMPRESSED_DATA_OBJECT_SIZE = 768 * 1024 * 1024;

export function compressLz4DataPayload(payload) {
  const src = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  const compressed = lz4.makeBuffer(lz4.compressBound(src.length));
  const hashTable = new Uint32Array(1 << 16);
  const compressedSize = lz4.compressBlock(src, compressed, 0, src.length, hashTable);
  if (compressedSize <= 0) return null;

  const out = Buffer.alloc(8 + compressedSize);
  out.writeBigUInt64LE(BigInt(src.length), 0);
  Buffer.from(compressed.buffer, compressed.byteOffset, compressedSize).copy(out, 8);
  return out;
}

export function decompressLz4DataPayload(payload) {
  const src = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  if (src.length < 8) {
    throw new Error('lz4 compressed payload too short');
  }

  const uncompressedSize = src.readBigUInt64LE(0);
  if (uncompressedSize > BigInt(MAX_UNCOMPRESSED_DATA_OBJECT_SIZE)) {
    throw new Error('lz4 decompressed payload too large');
  }
  if (uncompressedSize > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error('lz4 decompressed payload size is not representable');
  }

  const expectedSize = Number(uncompressedSize);
  const compressed = src.subarray(8);
  const out = Buffer.alloc(expectedSize);
  const decompressedSize = lz4.decompressBlock(compressed, out, 0, compressed.length, 0);
  if (decompressedSize !== expectedSize) {
    throw new Error('lz4 decompressed size mismatch');
  }
  return out;
}
