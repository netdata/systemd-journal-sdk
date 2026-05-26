// Directory writer (Log) for managing a journal directory with rotation and retention.

import { closeSync, existsSync, fsyncSync, mkdirSync, openSync, readSync, readdirSync, readFileSync, unlinkSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID, stringToUUID, uuidToString } from './binary.js';
import { Writer } from './writer.js';
import { HEADER_SIZE, STATE_ONLINE, parseFileHeader, parseObjectHeader } from './header.js';

const DEFAULT_MAX_ENTRIES = 0;
const DEFAULT_MAX_BYTES = 0;
const DEFAULT_MAX_DURATION_USEC = 0n;
const DEFAULT_MAX_FILES = 0;
const DEFAULT_RETENTION_BYTES = 0;
const DEFAULT_RETENTION_AGE_USEC = 0n;

export class Log {
  constructor(directory, options = {}) {
    if (!directory) throw new Error('invalid journal directory');
    this.rootDirectory = directory;
    this.source = options.source || 'system';
    validateJournalSource(this.source);
    this.strictSystemdNaming = options.strictSystemdNaming === true || options.strict_systemd_naming === true;

    // Rotation policy
    this.maxEntries = options.maxEntries ?? DEFAULT_MAX_ENTRIES;
    this.maxBytes = options.maxBytes ?? DEFAULT_MAX_BYTES;
    this.maxDurationUsec = optionUsec(
      options.maxDurationUsec ?? options.max_duration_usec,
      DEFAULT_MAX_DURATION_USEC,
    );

    // Retention policy
    this.maxFiles = options.maxFiles ?? DEFAULT_MAX_FILES;
    this.maxRetentionBytes = options.maxRetentionBytes ?? DEFAULT_RETENTION_BYTES;
    this.maxRetentionAgeUsec = optionUsec(
      options.maxRetentionAgeUsec ?? options.max_retention_age_usec,
      DEFAULT_RETENTION_AGE_USEC,
    );

    this.activePath = null;
    this.writer = null;
    this.closed = false;
    this._pathCounter = 0;
    this.nextSeqnum = options.headSeqnum ? BigInt(options.headSeqnum) : 1n;
    this.seqnumId = options.seqnumId ? Buffer.from(options.seqnumId) : randomUUID();
    this.bootId = options.bootId ? Buffer.from(options.bootId) : null;
    this.machineId = options.machineId ? Buffer.from(options.machineId) : readMachineId() || randomUUID();
    this.compression = options.compression ?? 'none';
    this.compressionThresholdBytes = options.compressionThresholdBytes;
    this.compact = options.compact === true || options.format === 'compact';
    this.directory = join(this.rootDirectory, uuidToString(this.machineId));

    this._ensureDirectory();
    const chainState = this._scanChainState();
    if (options.headSeqnum === undefined && chainState.tailSeqnum > 0n) this.nextSeqnum = chainState.tailSeqnum + 1n;
    if (!options.seqnumId && chainState.seqnumId) this.seqnumId = Buffer.from(chainState.seqnumId);
    if (!this.strictSystemdNaming) {
      if (chainState.activePath) this.activePath = chainState.activePath;
    }
    this._findOrCreateActiveFile();
  }

  _ensureDirectory() {
    mkdirSync(this.directory, { recursive: true });
  }

  _findOrCreateActiveFile() {
    if (this.strictSystemdNaming) this.activePath = this._systemdActivePath();
  }

  append(fields, options = {}) {
    if (this.closed) throw new Error('journal log is closed');
    if (fields.length === 0) throw new Error('empty entry');
    const appendOptions = this._entryOptionsForAppend(options);
    if (this.writer && this._shouldRotate(appendOptions.realtimeUsec)) {
      this._rotate(appendOptions);
    }
    if (!this.writer) {
      this._openWriter(appendOptions);
    }

    const result = this.writer.append(fields, appendOptions);
    this._captureWriterIdentity();
    return result;
  }

  _shouldRotate(nextRealtimeUsec) {
    if (!this.writer) return false;
    const entryCount = Number(this.writer.header.n_entries);
    const fileSize = Number(this.writer.appendOffset);
    return (this.maxEntries > 0 && entryCount >= this.maxEntries) ||
      (this.maxBytes > 0 && fileSize >= this.maxBytes) ||
      (
        this.maxDurationUsec > 0n &&
        this.writer.header.n_entries > 0n &&
        this.writer.header.head_entry_realtime > 0n &&
        BigInt(nextRealtimeUsec) >= this.writer.header.head_entry_realtime &&
        BigInt(nextRealtimeUsec) - this.writer.header.head_entry_realtime >= this.maxDurationUsec
      );
  }

  _rotate(options = {}) {
    if (!this.writer) return;

    this._captureWriterIdentity();

    const archivedPath = this.strictSystemdNaming ? this._archivePathFor(this.writer.header) : this.activePath;
    try {
      this.writer.archiveTo(archivedPath);
    } catch (error) {
      if (this.writer.closed) {
        this.writer = null;
        this.activePath = this.strictSystemdNaming ? this._systemdActivePath() : null;
      }
      throw error;
    }
    this.writer = null;

    this.activePath = this.strictSystemdNaming ? this._systemdActivePath() : null;
    this._openWriter(options);
    this._applyRetention(this.activePath);
  }

  _openWriter(options = {}) {
    if (!this.activePath) {
      const headRealtime = optionUsec(options.realtimeUsec ?? options.realtime_usec, nowUsec());
      this.activePath = this._chainPathFor(this.seqnumId, this.nextSeqnum, headRealtime);
    }
    if (existsSync(this.activePath)) {
      this.writer = Writer.open(this.activePath);
      if (this.writer.header.n_entries === 0n) {
        this._discardEmptyOpenedWriter();
        if (!this.activePath) {
          const headRealtime = optionUsec(options.realtimeUsec ?? options.realtime_usec, nowUsec());
          this.activePath = this._chainPathFor(this.seqnumId, this.nextSeqnum, headRealtime);
        }
      } else {
        this._captureWriterIdentity();
        return;
      }
    }

    if (existsSync(this.activePath)) {
      this.writer = Writer.open(this.activePath);
      this._captureWriterIdentity();
      return;
    }

    const opts = { headSeqnum: this.nextSeqnum, compression: this.compression, compact: this.compact };
    if (this.compressionThresholdBytes !== undefined) {
      opts.compressionThresholdBytes = this.compressionThresholdBytes;
    }
    if (this.seqnumId) opts.seqnumId = this.seqnumId;
    if (this.bootId) opts.bootId = this.bootId;
    if (this.machineId) opts.machineId = this.machineId;
    this.writer = Writer.create(this.activePath, opts);
    this._captureWriterIdentity();
  }

  _discardEmptyOpenedWriter() {
    this.writer.close();
    unlinkIfExists(this.activePath);
    this.writer = null;
    if (!this.strictSystemdNaming) this.activePath = null;
  }

  _captureWriterIdentity() {
    this.seqnumId = Buffer.from(this.writer.header.seqnum_id);
    this.bootId = Buffer.from(this.writer.bootId);
    this.machineId = Buffer.from(this.writer.header.machine_id);
    this.nextSeqnum = this.writer.nextSeqnum;
  }

  _systemdActivePath() {
    return join(this.directory, `${this.source}.journal`);
  }

  _chainPathFor(seqnumId, headSeqnum, headRealtime) {
    return join(
      this.directory,
      `${this.source}@${uuidToString(seqnumId)}-${hex64(headSeqnum)}-${hex64(headRealtime)}.journal`,
    );
  }

  _archivePathFor(header) {
    return this._chainPathFor(header.seqnum_id, header.head_entry_seqnum, header.head_entry_realtime);
  }

  _scanChainState() {
    const state = {
      tailSeqnum: 0n,
      seqnumId: null,
      activePath: null,
      activeTailSeqnum: 0n,
      activeHeadRealtime: 0n,
    };
    for (const name of readdirSync(this.directory)) {
      if (!parseArchivedJournalName(name, this.source)) continue;
      const path = join(this.directory, name);
      try {
        const header = readJournalHeader(path);
        if (header.tail_entry_seqnum > state.tailSeqnum) {
          state.tailSeqnum = header.tail_entry_seqnum;
          state.seqnumId = Buffer.from(header.seqnum_id);
        }
        if (
          header.state === STATE_ONLINE &&
          (state.activePath === null ||
            header.tail_entry_seqnum > state.activeTailSeqnum ||
            (header.tail_entry_seqnum === state.activeTailSeqnum &&
              header.head_entry_realtime > state.activeHeadRealtime))
        ) {
          state.activePath = path;
          state.activeTailSeqnum = header.tail_entry_seqnum;
          state.activeHeadRealtime = header.head_entry_realtime;
        }
      } catch {}
    }
    return state;
  }

  _applyRetention(protectedPath = this.activePath) {
    const entries = readdirSync(this.directory);
    const archives = [];
    for (const entry of entries) {
      const parsed = parseArchivedJournalName(entry, this.source);
      if (!parsed) continue;
      const fullPath = join(this.directory, parsed.name);
      try {
        const stat = statSync(fullPath);
        archives.push({
          name: parsed.name,
          path: fullPath,
          size: committedJournalSize(fullPath, stat.size),
          headSeqnum: parsed.headSeqnum,
          headRealtime: parsed.headRealtime,
        });
      } catch {}
    }
    archives.sort((a, b) => {
        if (a.headRealtime !== b.headRealtime) return a.headRealtime < b.headRealtime ? -1 : 1;
        if (a.headSeqnum !== b.headSeqnum) return a.headSeqnum < b.headSeqnum ? -1 : 1;
        return a.path.localeCompare(b.path);
      });

    const activePath = protectedPath;
    let activeInArchives = false;
    let totalBytes = 0;
    for (const archive of archives) {
      if (activePath && archive.path === activePath) activeInArchives = true;
      totalBytes += archive.size;
    }
    let activeExtraFile = false;
    try {
      if (activePath && !activeInArchives) {
        const stat = statSync(activePath);
        totalBytes += committedJournalSize(activePath, stat.size);
        activeExtraFile = true;
      }
    } catch {}

    // Remove excess files beyond maxFiles
    let fileCount = archives.length + (activeExtraFile ? 1 : 0);
    while (this.maxFiles > 0 && fileCount > this.maxFiles) {
      const oldestIndex = archives.findIndex((archive) => !activePath || archive.path !== activePath);
      if (oldestIndex === -1) break;
      const [oldest] = archives.splice(oldestIndex, 1);
      unlinkIfExists(oldest.path);
      totalBytes = Math.max(0, totalBytes - oldest.size);
      fileCount--;
    }

    // Check active plus archived size and remove oldest archives if over maxRetentionBytes.
    while (this.maxRetentionBytes > 0 && totalBytes > this.maxRetentionBytes && archives.length > 0) {
      const oldestIndex = archives.findIndex((archive) => !activePath || archive.path !== activePath);
      if (oldestIndex === -1) break;
      const [oldest] = archives.splice(oldestIndex, 1);
      totalBytes = Math.max(0, totalBytes - oldest.size);
      unlinkIfExists(oldest.path);
    }
    if (this.maxRetentionAgeUsec > 0n) {
      const cutoff = saturatingSubBigInt(nowUsec(), this.maxRetentionAgeUsec);
      while (archives.length > 0) {
        const oldestIndex = archives.findIndex((archive) => {
          if (archive.headRealtime > cutoff) return false;
          return !activePath || archive.path !== activePath;
        });
        if (oldestIndex === -1) break;
        const [oldest] = archives.splice(oldestIndex, 1);
        unlinkIfExists(oldest.path);
        totalBytes = Math.max(0, totalBytes - oldest.size);
      }
    }
    syncDirectory(this.directory);
  }

  enforceRetention() {
    if (this.closed) throw new Error('journal log is closed');
    this._applyRetention(this.activePath);
  }

  _entryOptionsForAppend(options) {
    const appendOptions = { ...options };
    if (appendOptions.realtimeUsec === undefined && appendOptions.realtime_usec !== undefined) {
      appendOptions.realtimeUsec = appendOptions.realtime_usec;
    }
    if (appendOptions.realtimeUsec === undefined || appendOptions.realtimeUsec === 0 || appendOptions.realtimeUsec === 0n) {
      appendOptions.realtimeUsec = nowUsec();
    }
    return appendOptions;
  }

  sync() {
    if (this.closed) throw new Error('journal log is closed');
    if (this.writer) {
      this.writer.sync();
    }
  }

  close() {
    if (this.closed) return;
    if (this.writer) {
      if (this.writer.header.n_entries === 0n && this.strictSystemdNaming) {
        try {
          this.writer.close();
          unlinkIfExists(this.activePath);
        } catch (error) {
          if (this.writer.closed) {
            this.writer = null;
            this.closed = true;
          }
          throw error;
        }
      } else {
        const archivedPath = this.strictSystemdNaming ? this._archivePathFor(this.writer.header) : this.activePath;
        try {
          this.writer.archiveTo(archivedPath);
        } catch (error) {
          if (this.writer.closed) {
            this.activePath = archivedPath;
            this.writer = null;
            this.closed = true;
          }
          throw error;
        }
        this.activePath = archivedPath;
        this.writer = null;
        this.closed = true;
        this._applyRetention(archivedPath);
        return;
      }
      this.writer = null;
    }
    this.closed = true;
  }

  activeFile() {
    return this.activePath || this._chainPathFor(this.seqnumId, this.nextSeqnum, 0n);
  }

  journalDirectory() {
    return this.directory;
  }
}

export default Log;

function validateJournalSource(source) {
  if (source === '' || source === '.' || source === '..') throw new Error('invalid journal source');
  for (let i = 0; i < source.length; i++) {
    const c = source.charCodeAt(i);
    const ok = (c >= 0x61 && c <= 0x7a) || (c >= 0x41 && c <= 0x5a) ||
      (c >= 0x30 && c <= 0x39) || c === 0x5f || c === 0x2d || c === 0x2e;
    if (!ok) throw new Error('invalid journal source');
  }
}

function readMachineId() {
  try {
    const text = readFileSync('/etc/machine-id', 'utf8').trim();
    if (/^[0-9a-fA-F]{32}$/.test(text)) return stringToUUID(text);
  } catch {}
  return null;
}

function readJournalHeader(path) {
  const fd = openSync(path, 'r');
  try {
    const headerBuf = Buffer.alloc(HEADER_SIZE);
    const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
    if (bytesRead < HEADER_SIZE) throw new Error('cannot read journal header');
    return parseFileHeader(headerBuf);
  } finally {
    closeSync(fd);
  }
}

function committedJournalSize(path, fallback) {
  const fd = openSync(path, 'r');
  try {
    const headerBuf = Buffer.alloc(HEADER_SIZE);
    const bytesRead = readSync(fd, headerBuf, 0, HEADER_SIZE, 0);
    if (bytesRead < HEADER_SIZE) return fallback;
    const header = parseFileHeader(headerBuf);
    if (header.tail_object_offset === 0n) return fallback;

    const objectBuf = Buffer.alloc(16);
    const objectBytes = readSync(fd, objectBuf, 0, objectBuf.length, Number(header.tail_object_offset));
    if (objectBytes < objectBuf.length) return fallback;
    const objectHeader = parseObjectHeader(objectBuf);
    if (!objectHeader) return fallback;
    return Number(align8BigInt(header.tail_object_offset + objectHeader.size));
  } catch {
    return fallback;
  } finally {
    closeSync(fd);
  }
}

function align8BigInt(value) {
  return (value + 7n) & ~7n;
}

function unlinkIfExists(path) {
  try {
    unlinkSync(path);
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error;
  }
}

function parseArchivedJournalName(name, source) {
  if (!name.endsWith('.journal')) return null;
  const stem = name.slice(0, -'.journal'.length);
  const prefix = `${source}@`;
  if (!stem.startsWith(prefix)) return null;
  const parts = stem.slice(prefix.length).split('-');
  if (parts.length !== 3) return null;
  if (!/^[0-9a-fA-F]{32}$/.test(parts[0])) return null;
  if (!/^[0-9a-fA-F]{16}$/.test(parts[1]) || !/^[0-9a-fA-F]{16}$/.test(parts[2])) return null;
  return {
    name,
    headSeqnum: BigInt(`0x${parts[1]}`),
    headRealtime: BigInt(`0x${parts[2]}`),
  };
}

function hex64(value) {
  return BigInt(value).toString(16).padStart(16, '0');
}

function nowUsec() {
  return BigInt(Date.now()) * 1000n;
}

function optionUsec(value, fallback) {
  if (value === undefined || value === null) return BigInt(fallback);
  return BigInt(value);
}

function saturatingSubBigInt(value, amount) {
  return value >= amount ? value - amount : 0n;
}

function syncDirectory(path) {
  const fd = openSync(path, 'r');
  try {
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}
