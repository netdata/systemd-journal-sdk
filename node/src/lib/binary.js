// Low-level binary helpers for systemd journal file parsing/writing.
// All 64-bit journal values use BigInt internally.

import { randomFillSync } from 'node:crypto';

// Read a uint64 from a Buffer as a BigInt.
export function readUint64LE(buf, offset = 0) {
  const low = buf.readUInt32LE(offset);
  const high = buf.readUInt32LE(offset + 4);
  return (BigInt(high) << 32n) + BigInt(low);
}

// Write a BigInt (uint64) to a Buffer in little-endian.
export function writeUint64LE(buf, offset, value) {
  const v = BigInt(value);
  buf.writeUInt32LE(Number(v & 0xffffffffn), offset);
  buf.writeUInt32LE(Number(v >> 32n), offset + 4);
}

// Write a uint32 to a Buffer.
export function writeUint32LE(buf, offset, value) {
  buf.writeUInt32LE(value, offset);
}

// Write a uint8 to a Buffer.
export function writeUint8(buf, offset, value) {
  buf.writeUInt8(value, offset);
}

// Align a value up to the next multiple of 8 (matches Go align8).
export function align8(value) {
  const v = BigInt(value);
  return (v + 7n) & ~7n;
}

// Compare two buffers for equality.
export function bufEqual(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

// Format a UUID Buffer as a 32-character lowercase hex string.
export function uuidToString(uuid) {
  return uuid.toString('hex');
}

// Parse a 32-character hex string into a UUID Buffer.
export function stringToUUID(hex) {
  return Buffer.from(hex, 'hex');
}

// Check if a UUID is all zeros.
export function isZeroUUID(uuid) {
  for (let i = 0; i < 16; i++) {
    if (uuid[i] !== 0) return false;
  }
  return true;
}

// Generate a random UUID-shaped identifier (16 bytes).
export function randomUUID() {
  const buf = Buffer.alloc(16);
  randomFillSync(buf);
  return buf;
}
