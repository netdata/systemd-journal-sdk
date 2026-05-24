// Journal entry and object parsing helpers.

import { readUint64LE } from './binary.js';
import {
  OBJECT_TYPE_ENTRY, OBJECT_TYPE_DATA, OBJECT_HEADER_SIZE,
  ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
  OBJECT_COMPRESSED_ZSTD, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4,
} from './header.js';
import { zstdDecompressSync } from 'node:zlib';

// Parse an entry object from a buffer at offset.
// Returns { seqnum, realtime, monotonic, boot_id, xor_hash, items: [{offset, hash}] }.
export function parseEntryObject(buf, offset) {
  const objType = buf.readUInt8(offset);
  if (objType !== OBJECT_TYPE_ENTRY) {
    throw new Error(`expected ENTRY (type ${OBJECT_TYPE_ENTRY}), got type ${objType} at offset ${offset}`);
  }
  const objSize = readUint64LE(buf, offset + 8);
  if (objSize < BigInt(ENTRY_OBJECT_HEADER_SIZE)) {
    throw new Error(`entry object too small: ${objSize}`);
  }

  const eOff = offset + OBJECT_HEADER_SIZE;
  const seqnum = readUint64LE(buf, eOff);
  const realtime = readUint64LE(buf, eOff + 8);
  const monotonic = readUint64LE(buf, eOff + 16);
  const boot_id = Buffer.from(buf.slice(eOff + 24, eOff + 40));
  const xor_hash = readUint64LE(buf, eOff + 40);

  // Data items follow the entry header
  const itemsStart = offset + ENTRY_OBJECT_HEADER_SIZE;
  const nItems = Number((objSize - BigInt(ENTRY_OBJECT_HEADER_SIZE)) / BigInt(16));
  const items = [];
  for (let i = 0; i < nItems; i++) {
    const iOff = itemsStart + i * 16;
    const dataOffset = readUint64LE(buf, iOff);
    const dataHash = readUint64LE(buf, iOff + 8);
    if (dataOffset !== 0n) {
      items.push({ offset: dataOffset, hash: dataHash });
    }
  }

  return { seqnum, realtime, monotonic, boot_id, xor_hash, items };
}

// Parse a DATA object from buffer at offset.
// Returns { name: Buffer, value: Buffer }.
export function parseDataObject(buf, offset) {
  if (buf.length < offset + DATA_OBJECT_HEADER_SIZE) {
    throw new Error('buffer too small for data object');
  }
  const objType = buf.readUInt8(offset);
  const objFlags = buf.readUInt8(offset + 1);
  const objSize = readUint64LE(buf, offset + 8);

  if (objType !== OBJECT_TYPE_DATA) {
    throw new Error(`expected DATA (type ${OBJECT_TYPE_DATA}), got type ${objType}`);
  }
  if (objSize < BigInt(DATA_OBJECT_HEADER_SIZE)) {
    throw new Error(`data object too small: ${objSize}`);
  }

  let payload = buf.slice(offset + DATA_OBJECT_HEADER_SIZE, offset + Number(objSize));

  // Decompress if needed
  if ((objFlags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)) !== 0) {
    throw new Error(`unsupported DATA object flags: 0x${objFlags.toString(16)}`);
  }
  const unsupportedCompression = objFlags & (OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4);
  if (unsupportedCompression !== 0) {
    throw new Error(`unsupported DATA object compression flags: 0x${objFlags.toString(16)}`);
  }
  if (objFlags & OBJECT_COMPRESSED_ZSTD) {
    payload = zstdDecompressSync(payload);
  }

  const eqPos = payload.indexOf(0x3d);
  if (eqPos < 0) throw new Error('DATA object missing field separator');

  return {
    name: payload.slice(0, eqPos),
    value: payload.slice(eqPos + 1),
  };
}
