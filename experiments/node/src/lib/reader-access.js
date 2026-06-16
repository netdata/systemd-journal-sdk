import { closeSync, fstatSync, openSync, readSync } from 'node:fs';

export const READER_ACCESS_AUTO = 'auto';
export const READER_ACCESS_READ_AT = 'read-at';
export const READER_ACCESS_MMAP = 'mmap';
export const READER_BOUNDS_LIVE = 'live';
export const READER_BOUNDS_SNAPSHOT = 'snapshot';

export const DEFAULT_WINDOW_SIZE_BYTES = 32 * 1024 * 1024;
export const DEFAULT_MAX_WINDOWS = 4;
export const DEFAULT_MAX_ROW_ARENA_BYTES = 256 * 1024 * 1024;
export const DEFAULT_ROW_ARENA_SEGMENT_BYTES = 1024 * 1024;

const EMPTY_BUFFER = Buffer.alloc(0);

export class UnsupportedAccessModeError extends Error {
  constructor(mode) {
    super(`unsupported reader access mode for Node.js core: ${mode}`);
    this.name = 'UnsupportedAccessModeError';
    this.accessMode = mode;
  }
}

export function normalizeReaderOptions(options = {}) {
  const source = options ?? {};
  const accessMode = normalizeAccessMode(source.accessMode ?? source.mmapStrategy ?? READER_ACCESS_AUTO);
  const bounds = normalizeBounds(source.bounds ?? READER_BOUNDS_LIVE);
  const windowSizeBytes = normalizePositiveInteger(
    source.windowSizeBytes ?? source.windowSize ?? DEFAULT_WINDOW_SIZE_BYTES,
    'windowSizeBytes',
  );
  const maxWindows = normalizePositiveInteger(source.maxWindows ?? DEFAULT_MAX_WINDOWS, 'maxWindows');
  const maxRowArenaBytes = normalizeNonNegativeInteger(
    source.maxRowArenaBytes ?? DEFAULT_MAX_ROW_ARENA_BYTES,
    'maxRowArenaBytes',
  );
  const rowArenaSegmentBytes = normalizePositiveInteger(
    source.rowArenaSegmentBytes ?? DEFAULT_ROW_ARENA_SEGMENT_BYTES,
    'rowArenaSegmentBytes',
  );
  const zstdTimeoutMs = normalizePositiveInteger(source.zstdTimeoutMs ?? 600000, 'zstdTimeoutMs');

  return {
    accessMode,
    bounds,
    windowSizeBytes,
    maxWindows,
    maxRowArenaBytes,
    rowArenaSegmentBytes,
    zstdTimeoutMs,
  };
}

export function withSnapshotBounds(options = {}) {
  return { ...normalizeReaderOptions(options), bounds: READER_BOUNDS_SNAPSHOT };
}

export function openReaderAccessor(path, options = {}) {
  const opts = normalizeReaderOptions(options);
  if (opts.accessMode === READER_ACCESS_MMAP) {
    throw new UnsupportedAccessModeError(opts.accessMode);
  }
  const fd = openSync(path, 'r');
  try {
    return new ReadAtAccessor(fd, opts);
  } catch (error) {
    try { closeSync(fd); } catch {
      // Preserve the original open failure.
    }
    throw error;
  }
}

function normalizeAccessMode(value) {
  const text = String(value ?? READER_ACCESS_AUTO).toLowerCase().replaceAll('_', '-');
  if (text === 'auto' || text === 'buffer') return READER_ACCESS_AUTO;
  if (text === 'readat' || text === 'read-at' || text === 'pread') return READER_ACCESS_READ_AT;
  if (text === 'mmap') return READER_ACCESS_MMAP;
  throw new Error(`unsupported reader access mode: ${value}`);
}

function normalizeBounds(value) {
  const text = String(value ?? READER_BOUNDS_LIVE).toLowerCase().replaceAll('_', '-');
  if (text === READER_BOUNDS_LIVE) return READER_BOUNDS_LIVE;
  if (text === READER_BOUNDS_SNAPSHOT) return READER_BOUNDS_SNAPSHOT;
  throw new Error(`unsupported reader bounds mode: ${value}`);
}

function normalizePositiveInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number <= 0) {
    throw new Error(`${name} must be a positive safe integer`);
  }
  return number;
}

function normalizeNonNegativeInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0) {
    throw new Error(`${name} must be a non-negative safe integer`);
  }
  return number;
}

class ReadAtAccessor {
  constructor(fd, options) {
    this.fd = fd;
    this.options = options;
    this.visibleSize = fstatSync(fd).size;
    this.windows = new Map();
    this.rowArena = new RowArena(options.maxRowArenaBytes, options.rowArenaSegmentBytes);
    this.closed = false;
    this.scratch = Buffer.alloc(8);

    this.statsValue = {
      requestedAccessMode: options.accessMode,
      selectedAccessMode: READER_ACCESS_READ_AT,
      selectedBackend: READER_ACCESS_READ_AT,
      fallbackReason: options.accessMode === READER_ACCESS_AUTO ? 'Node.js core has no mmap API' : '',
      bounds: options.bounds,
      visibleSize: this.visibleSize,
      windowSizeBytes: options.windowSizeBytes,
      maxWindows: options.maxWindows,
      windowsCreated: 0,
      windowHits: 0,
      windowMisses: 0,
      evictions: 0,
      pinnedWindows: 0,
      readBufferBytes: 0,
      rowArenaPeakBytes: 0,
      tempCopyBytes: 0,
      tempCopyCount: 0,
      shortReads: 0,
      readSyncUsesPosition: true,
    };
  }

  size() {
    return this.visibleSize;
  }

  stats() {
    this.statsValue.visibleSize = this.visibleSize;
    this.statsValue.pinnedWindows = Array.from(this.windows.values()).filter((window) => window.rowPinned).length;
    this.statsValue.readBufferBytes = Array.from(this.windows.values())
      .reduce((total, window) => total + window.buffer.length, 0);
    this.statsValue.rowArenaPeakBytes = this.rowArena.peakBytes;
    return { ...this.statsValue };
  }

  snapshotVisibleBounds() {
    return this.visibleSize;
  }

  restoreVisibleBounds(size) {
    this.visibleSize = Number(size);
  }

  refreshVisibleBounds() {
    if (this.options.bounds === READER_BOUNDS_SNAPSHOT) return false;
    const nextSize = fstatSync(this.fd).size;
    const changed = nextSize !== this.visibleSize;
    this.visibleSize = nextSize;
    this._dropUnpinnedWindows();
    return changed;
  }

  u8(offset) {
    return this.tempView(offset, 1)[0];
  }

  u32(offset) {
    return this.tempView(offset, 4).readUInt32LE(0);
  }

  u64(offset) {
    return this.tempView(offset, 8).readBigUInt64LE(0);
  }

  readBytes(offset, size) {
    return Buffer.from(this.tempView(offset, size));
  }

  tempView(offset, size) {
    return this._view(offset, size, false);
  }

  rowView(offset, size) {
    return this._view(offset, size, true);
  }

  rowBytes(data) {
    return this.rowArena.append(data);
  }

  updateHmac(hmac, offset, size, chunkSize = 1 << 20) {
    let pos = Number(offset);
    let remaining = Number(size);
    if (remaining < 0) throw new Error('negative HMAC range size');
    while (remaining > 0) {
      const chunk = Math.min(remaining, chunkSize);
      hmac.update(this.tempView(pos, chunk));
      pos += chunk;
      remaining -= chunk;
    }
  }

  clearRow() {
    for (const window of this.windows.values()) window.rowPinned = false;
    this.rowArena.clear();
    this._evictToBudget();
  }

  close() {
    if (this.closed) return;
    this.clearRow();
    this.windows.clear();
    closeSync(this.fd);
    this.closed = true;
  }

  _view(offset, size, row) {
    const { pos, len } = this._checkRange(offset, size);
    if (len === 0) return EMPTY_BUFFER;
    if (len > this.options.windowSizeBytes) {
      const copy = this._scratchRead(pos, len, row);
      return row ? this.rowBytes(copy) : copy;
    }

    const base = Math.floor(pos / this.options.windowSizeBytes) * this.options.windowSizeBytes;
    if (pos + len > base + this.options.windowSizeBytes) {
      const copy = this._scratchRead(pos, len, row);
      return row ? this.rowBytes(copy) : copy;
    }
    const window = this._windowFor(base, pos, len, row);
    const start = pos - window.base;
    if (row) window.rowPinned = true;
    return window.buffer.subarray(start, start + len);
  }

  _checkRange(offset, size) {
    const pos = Number(offset);
    const len = Number(size);
    if (!Number.isSafeInteger(pos) || !Number.isSafeInteger(len) || pos < 0 || len < 0) {
      throw new Error('invalid read range');
    }
    if (pos + len > this.visibleSize) {
      throw new Error('read exceeds visible file bounds');
    }
    return { pos, len };
  }

  _windowFor(base, offset, size, row) {
    const existing = this.windows.get(base);
    if (existing && offset + size <= existing.base + existing.buffer.length) {
      this.windows.delete(base);
      this.windows.set(base, existing);
      this.statsValue.windowHits++;
      return existing;
    }
    if (existing?.rowPinned) {
      if (row) return { base: offset, buffer: this.rowBytes(this._scratchRead(offset, size, true)), rowPinned: true };
      return { base: offset, buffer: this._scratchRead(offset, size, false), rowPinned: false };
    }
    if (this.windows.size >= this.options.maxWindows && this._allWindowsPinned()) {
      if (row) return { base: offset, buffer: this.rowBytes(this._scratchRead(offset, size, true)), rowPinned: true };
      return { base: offset, buffer: this._scratchRead(offset, size, false), rowPinned: false };
    }
    if (existing) {
      this.windows.delete(base);
      this.statsValue.evictions++;
    } else {
      this._evictToBudget(1);
    }
    const length = Math.min(this.options.windowSizeBytes, this.visibleSize - base);
    const buffer = Buffer.alloc(length);
    const bytes = readSync(this.fd, buffer, 0, length, base);
    if (bytes !== length) {
      this.statsValue.shortReads++;
      throw new Error('short read before visible file size');
    }
    const window = { base, buffer, rowPinned: false };
    this.windows.set(base, window);
    this.statsValue.windowsCreated++;
    this.statsValue.windowMisses++;
    this._evictToBudget();
    return window;
  }

  _scratchRead(offset, size, stable) {
    const buffer = size <= this.scratch.length ? this.scratch.subarray(0, size) : Buffer.alloc(size);
    const bytes = readSync(this.fd, buffer, 0, size, offset);
    if (bytes !== size) {
      this.statsValue.shortReads++;
      throw new Error('short read before visible file size');
    }
    this.statsValue.tempCopyBytes += size;
    this.statsValue.tempCopyCount++;
    return stable && size <= this.scratch.length ? Buffer.from(buffer) : buffer;
  }

  _allWindowsPinned() {
    return this.windows.size > 0 && Array.from(this.windows.values()).every((window) => window.rowPinned);
  }

  _dropUnpinnedWindows() {
    for (const [base, window] of this.windows) {
      if (!window.rowPinned) {
        this.windows.delete(base);
        this.statsValue.evictions++;
      }
    }
  }

  _evictToBudget(extraNeeded = 0) {
    while (this.windows.size + extraNeeded > this.options.maxWindows) {
      let evicted = false;
      for (const [base, window] of this.windows) {
        if (!window.rowPinned) {
          this.windows.delete(base);
          this.statsValue.evictions++;
          evicted = true;
          break;
        }
      }
      if (!evicted) return;
    }
  }
}

class RowArena {
  constructor(limitBytes, segmentBytes) {
    this.limitBytes = limitBytes;
    this.segmentBytes = segmentBytes;
    this.segments = [];
    this.currentBytes = 0;
    this.peakBytes = 0;
    this.usedInCurrent = 0;
  }

  append(data) {
    const src = bufferView(data);
    if (this.currentBytes + src.length > this.limitBytes) {
      throw new Error('row arena limit exceeded');
    }
    if (this.segments.length === 0 || this._remaining() < src.length) {
      this.segments.push(Buffer.alloc(Math.max(this.segmentBytes, src.length)));
      this.usedInCurrent = 0;
    }
    const segment = this.segments[this.segments.length - 1];
    const start = this.usedInCurrent;
    src.copy(segment, start);
    this.usedInCurrent += src.length;
    this.currentBytes += src.length;
    this.peakBytes = Math.max(this.peakBytes, this.currentBytes);
    return segment.subarray(start, start + src.length);
  }

  clear() {
    this.segments = [];
    this.currentBytes = 0;
    this.usedInCurrent = 0;
  }

  _remaining() {
    if (this.segments.length === 0) return 0;
    return this.segments[this.segments.length - 1].length - this.usedInCurrent;
  }
}

function bufferView(data) {
  if (Buffer.isBuffer(data)) return data;
  if (data instanceof Uint8Array) {
    return Buffer.from(data.buffer, data.byteOffset, data.byteLength);
  }
  return Buffer.from(data);
}
