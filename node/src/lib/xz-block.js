import { readFileSync } from 'node:fs';
import createLZMA from '../../vendor/node-liblzma-wasm/liblzma.js';

const LZMA_OK = 0;
const LZMA_BUF_ERROR = 10;
const LZMA_CHECK_NONE = 0;
const LZMA_PRESET_DEFAULT = 0;

export const MAX_UNCOMPRESSED_DATA_OBJECT_SIZE = 768 * 1024 * 1024;

const wasmUrl = new URL('../../vendor/node-liblzma-wasm/liblzma.wasm', import.meta.url);
const wasmBytes = readFileSync(wasmUrl);
const wasmBinary = wasmBytes.buffer.slice(
  wasmBytes.byteOffset,
  wasmBytes.byteOffset + wasmBytes.byteLength,
);
const wasmModule = await createLZMA({ wasmBinary });

function checkedMalloc(size) {
  const ptr = wasmModule._malloc(size);
  if (ptr === 0) throw new Error('xz: WASM memory allocation failed');
  return ptr;
}

export function compressXzDataPayload(payload) {
  const src = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  if (src.length < 80) return null;

  let inPtr = 0;
  let outPtr = 0;
  let outPosPtr = 0;

  try {
    inPtr = checkedMalloc(src.length);
    wasmModule.HEAPU8.set(src, inPtr);

    const outSize = src.length + 1024;
    outPtr = checkedMalloc(outSize);
    outPosPtr = checkedMalloc(4);
    wasmModule.HEAP32[outPosPtr >> 2] = 0;

    const ret = wasmModule._lzma_easy_buffer_encode(
      LZMA_PRESET_DEFAULT, LZMA_CHECK_NONE,
      0, inPtr, src.length, outPtr, outPosPtr, outSize,
    );
    if (ret !== LZMA_OK) return null;
    const outPos = wasmModule.HEAP32[outPosPtr >> 2];
    if (outPos >= src.length) return null;
    return Buffer.from(wasmModule.HEAPU8.subarray(outPtr, outPtr + outPos));
  } catch (e) {
    return null;
  } finally {
    if (inPtr !== 0) wasmModule._free(inPtr);
    if (outPtr !== 0) wasmModule._free(outPtr);
    if (outPosPtr !== 0) wasmModule._free(outPosPtr);
  }
}

export function decompressXzDataPayload(payload) {
  const src = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);

  let inPtr = 0;
  let inPosPtr = 0;
  let memlimitPtr = 0;
  let outPtr = 0;
  let outPosPtr = 0;

  try {
    inPtr = checkedMalloc(src.length);
    wasmModule.HEAPU8.set(src, inPtr);
    inPosPtr = checkedMalloc(4);
    memlimitPtr = checkedMalloc(8);

    let outSize = src.length * 4;
    if (outSize < 4096) outSize = 4096;
    outPtr = checkedMalloc(outSize);
    outPosPtr = checkedMalloc(4);

    const limit = BigInt(MAX_UNCOMPRESSED_DATA_OBJECT_SIZE);
    wasmModule.HEAP32[memlimitPtr >> 2] = Number(limit & 0xFFFFFFFFn);
    wasmModule.HEAP32[(memlimitPtr >> 2) + 1] = Number(limit >> 32n);

    for (let attempt = 0; attempt < 5; attempt++) {
      wasmModule.HEAP32[inPosPtr >> 2] = 0;
      wasmModule.HEAP32[outPosPtr >> 2] = 0;
      const ret = wasmModule._lzma_stream_buffer_decode(
        memlimitPtr, 0, 0, inPtr, inPosPtr, src.length, outPtr, outPosPtr, outSize,
      );
      if (ret === LZMA_OK) {
        const outPos = wasmModule.HEAP32[outPosPtr >> 2];
        return Buffer.from(wasmModule.HEAPU8.subarray(outPtr, outPtr + outPos));
      }
      if (ret === LZMA_BUF_ERROR) {
        const nextSize = outSize * 4;
        if (nextSize > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE) {
          throw new Error('xz decompression failed: output exceeds maximum data object size');
        }
        wasmModule._free(outPtr);
        outPtr = 0;
        outSize = nextSize;
        outPtr = checkedMalloc(outSize);
        continue;
      }
      throw new Error(`xz decompression failed with code ${ret}`);
    }
    throw new Error('xz decompression failed: output buffer exhausted');
  } finally {
    if (inPtr !== 0) wasmModule._free(inPtr);
    if (outPtr !== 0) wasmModule._free(outPtr);
    if (inPosPtr !== 0) wasmModule._free(inPosPtr);
    if (outPosPtr !== 0) wasmModule._free(outPosPtr);
    if (memlimitPtr !== 0) wasmModule._free(memlimitPtr);
  }
}
