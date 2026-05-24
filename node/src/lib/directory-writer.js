// Directory writer (Log) for managing a journal directory with rotation and retention.

import { closeSync, existsSync, fsyncSync, mkdirSync, openSync, readdirSync, readFileSync, unlinkSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID, stringToUUID, uuidToString } from './binary.js';
import { Writer } from './writer.js';

const DEFAULT_MAX_ENTRIES = 100000;
const DEFAULT_MAX_BYTES = 128 * 1024 * 1024; // 128 MiB
const DEFAULT_MAX_FILES = 10;
const DEFAULT_RETENTION_BYTES = 1024 * 1024 * 1024; // 1 GiB

export class Log {
  constructor(directory, options = {}) {
    if (!directory) throw new Error('invalid journal directory');
    this.rootDirectory = directory;
    this.source = options.source || 'system';
    validateJournalSource(this.source);

    // Rotation policy
    this.maxEntries = options.maxEntries || DEFAULT_MAX_ENTRIES;
    this.maxBytes = options.maxBytes || DEFAULT_MAX_BYTES;

    // Retention policy
    this.maxFiles = options.maxFiles || DEFAULT_MAX_FILES;
    this.maxRetentionBytes = options.maxRetentionBytes || DEFAULT_RETENTION_BYTES;

    this.activePath = null;
    this.writer = null;
    this.closed = false;
    this._pathCounter = 0;
    this.nextSeqnum = options.headSeqnum ? BigInt(options.headSeqnum) : 1n;
    this.seqnumId = options.seqnumId ? Buffer.from(options.seqnumId) : null;
    this.bootId = options.bootId ? Buffer.from(options.bootId) : null;
    this.machineId = options.machineId ? Buffer.from(options.machineId) : readMachineId() || randomUUID();
    this.compression = options.compression ?? 'none';
    this.compressionThresholdBytes = options.compressionThresholdBytes;
    this.compact = options.compact === true || options.format === 'compact';
    this.directory = join(this.rootDirectory, uuidToString(this.machineId));

    this._ensureDirectory();
    this._findOrCreateActiveFile();
  }

  _ensureDirectory() {
    mkdirSync(this.directory, { recursive: true });
  }

  _findOrCreateActiveFile() {
    this.activePath = this._newActivePath();
  }

  append(fields, options = {}) {
    if (this.closed) throw new Error('journal log is closed');
    if (!this.writer) {
      this._openWriter();
    }

    const result = this.writer.append(fields, options);

    // Check rotation conditions
    const entryCount = Number(this.writer.header.n_entries);
    const fileSize = Number(this.writer.appendOffset);

    if (entryCount >= this.maxEntries || fileSize >= this.maxBytes) {
      this._rotate();
    }

    return result;
  }

  _rotate() {
    if (!this.writer) return;

    this.nextSeqnum = this.writer.nextSeqnum;
    this.seqnumId = Buffer.from(this.writer.header.seqnum_id);
    this.bootId = Buffer.from(this.writer.bootId);
    this.machineId = Buffer.from(this.writer.header.machine_id);

    const archivedPath = this._archivePathFor(this.writer.header);
    this.writer.archiveTo(archivedPath);
    this.writer = null;

    // Apply retention policy
    this._applyRetention();

    // Create new active file
    this.activePath = this._newActivePath();
  }

  _openWriter() {
    if (existsSync(this.activePath)) {
      this.writer = Writer.open(this.activePath);
      this.nextSeqnum = this.writer.nextSeqnum;
      this.seqnumId = Buffer.from(this.writer.header.seqnum_id);
      this.bootId = Buffer.from(this.writer.bootId);
      this.machineId = Buffer.from(this.writer.header.machine_id);
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
    this.seqnumId = Buffer.from(this.writer.header.seqnum_id);
    this.bootId = Buffer.from(this.writer.bootId);
    this.machineId = Buffer.from(this.writer.header.machine_id);
  }

  _newActivePath() {
    return join(this.directory, `${this.source}.journal`);
  }

  _archivePathFor(header) {
    return join(
      this.directory,
      `${this.source}@${uuidToString(header.seqnum_id)}-${hex64(header.head_entry_seqnum)}-${hex64(header.head_entry_realtime)}.journal`,
    );
  }

  _applyRetention() {
    const entries = readdirSync(this.directory);
    const archives = entries
      .map(n => parseArchivedJournalName(n, this.source))
      .filter(Boolean)
      .map(n => {
        const fullPath = join(this.directory, n.name);
        return {
          name: n.name,
          path: fullPath,
          stat: statSync(fullPath),
          headSeqnum: n.headSeqnum,
          headRealtime: n.headRealtime,
        };
      })
      .sort((a, b) => {
        if (a.headRealtime !== b.headRealtime) return a.headRealtime < b.headRealtime ? -1 : 1;
        if (a.headSeqnum !== b.headSeqnum) return a.headSeqnum < b.headSeqnum ? -1 : 1;
        return a.path.localeCompare(b.path);
      });

    let totalBytes = archives.reduce((sum, f) => sum + f.stat.size, 0);
    try {
      totalBytes += statSync(this.activePath).size;
    } catch {}

    // Remove excess files beyond maxFiles
    while (archives.length > this.maxFiles) {
      const oldest = archives.shift();
      unlinkSync(oldest.path);
      totalBytes = Math.max(0, totalBytes - oldest.stat.size);
    }

    // Check active plus archived size and remove oldest archives if over maxRetentionBytes.
    while (totalBytes > this.maxRetentionBytes && archives.length > 0) {
      const oldest = archives.shift();
      totalBytes -= oldest.stat.size;
      unlinkSync(oldest.path);
    }
    syncDirectory(this.directory);
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
      if (this.writer.header.n_entries === 0n) {
        this.writer.close();
        try { unlinkSync(this.activePath); } catch {}
      } else {
        this.writer.archiveTo(this._archivePathFor(this.writer.header));
        this._applyRetention();
      }
      this.writer = null;
    }
    this.closed = true;
  }

  activeFile() {
    return this.activePath;
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

function syncDirectory(path) {
  const fd = openSync(path, 'r');
  try {
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}
