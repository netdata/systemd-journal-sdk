import { FileReader } from './reader.js';

export function byteSourceFromBuffer(buffer) {
  const data = Buffer.from(buffer);
  return new BufferByteSource(data);
}

export function byteSourceFromReader(reader) {
  return new ReaderByteSource(reader);
}

export function openVerificationByteSource(path, options = {}) {
  const reader = FileReader.open(path, options);
  return { source: byteSourceFromReader(reader), reader };
}

class BufferByteSource {
  constructor(buffer) {
    this.buffer = buffer;
    this.length = buffer.length;
  }

  u8(offset) {
    return this.buffer.readUInt8(offset);
  }

  u32(offset) {
    return this.buffer.readUInt32LE(offset);
  }

  u64(offset) {
    return this.buffer.readBigUInt64LE(offset);
  }

  bytes(offset, length) {
    return Buffer.from(this.buffer.subarray(offset, offset + length));
  }

  view(offset, length) {
    return this.buffer.subarray(offset, offset + length);
  }

  updateHmac(hmac, offset, length, chunkSize = 1 << 20) {
    let pos = Number(offset);
    let remaining = Number(length);
    while (remaining > 0) {
      const chunk = Math.min(remaining, chunkSize);
      hmac.update(this.view(pos, chunk));
      pos += chunk;
      remaining -= chunk;
    }
  }
}

class ReaderByteSource {
  constructor(reader) {
    this.reader = reader;
  }

  get length() {
    return this.reader._visibleSize();
  }

  u8(offset) {
    return this.reader._u8(offset);
  }

  u32(offset) {
    return this.reader._u32(offset);
  }

  u64(offset) {
    return this.reader._u64(offset);
  }

  bytes(offset, length) {
    return this.reader._readBytes(offset, length);
  }

  view(offset, length) {
    return this.reader.accessor.tempView(Number(offset), Number(length));
  }

  updateHmac(hmac, offset, length, chunkSize = 1 << 20) {
    this.reader.accessor.updateHmac(hmac, offset, length, chunkSize);
  }
}
