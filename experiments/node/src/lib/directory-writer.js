// Directory writer (Log) for managing a journal directory with rotation and retention.

import { closeSync, fsyncSync, readSync } from 'node:fs';
import { join } from 'node:path';
import { isZeroUUID, randomUUID, stringToUUID, uuidToString } from './binary.js';
import { Writer, normalizeFieldNamePolicy, normalizeFileMode, prepareFieldsForPolicy, prepareRawPayloadsForPolicy, writerPolicyForLogPolicy } from './writer.js';
import { HEADER_SIZE, STATE_ONLINE, parseFileHeader, parseObjectHeader, normalizeJournalMaxFileSize } from './header.js';
import {
  safeExistsSync,
  safeMkdirSync,
  safeOpenSync,
  safeReaddirSync,
  safeRenameSync,
  safeStatSync,
  safeUnlinkSync,
} from './fs-safe.js';

const DEFAULT_MAX_ENTRIES = 0;
const DEFAULT_MAX_BYTES = 0;
const DEFAULT_MAX_DURATION_USEC = 0n;
const DEFAULT_MAX_FILES = 0;
const DEFAULT_RETENTION_BYTES = 0;
const DEFAULT_RETENTION_AGE_USEC = 0n;
const DERIVED_ROTATION_FRACTION = 20;

export const LOG_OPEN_LAZY = 'lazy';
export const LOG_OPEN_EAGER = 'eager';
export const LOG_IDENTITY_AUTO = 'auto';
export const LOG_IDENTITY_STRICT = 'strict';
export const LOG_LIFECYCLE_CREATED = 'created';
export const LOG_LIFECYCLE_ROTATED = 'rotated';
export const LOG_LIFECYCLE_DELETED = 'deleted';
export const LOG_LIFECYCLE_REASON_APPEND = 'append';
export const LOG_LIFECYCLE_REASON_EAGER_OPEN = 'eager_open';
export const LOG_LIFECYCLE_REASON_ROTATION = 'rotation';
export const LOG_LIFECYCLE_REASON_RETENTION = 'retention';

export class Log {
  constructor(directory, options = {}) {
    if (!directory) throw new Error('invalid journal directory');
    this.rootDirectory = directory;
    this._configureGeneralOptions(options);
    this._configureRotation(options);
    this._configureRetention(options);
    this._deriveRotationFromRetention();
    this._initializeRuntimeState(options);
    this._initializeIdentity(options);

    this._ensureDirectory();
    this._attachExistingChainState(options);
    this._findOrCreateActiveFile();
    if (this.openMode === LOG_OPEN_EAGER && !this.writer) {
      this._openWriter({ realtimeUsec: nowUsec() }, LOG_LIFECYCLE_REASON_EAGER_OPEN);
    }
    this._applyRetentionOnOpen();
  }

  _configureGeneralOptions(options) {
    this.source = options.source || 'system';
    validateJournalSource(this.source);
    this.strictSystemdNaming = options.strictSystemdNaming === true || options.strict_systemd_naming === true;
    this.openMode = normalizeOpenMode(options);
    this.identityMode = normalizeIdentityMode(options);
    this.lifecycle = normalizeLifecycle(optionValue(options, 'lifecycle', 'lifecycleObserver', 'lifecycle_observer'));
    this.lifecycleErrorHandler = optionValue(options, 'lifecycleErrorHandler', 'lifecycle_error_handler');
    this.artifactSizer = normalizeArtifactSizer(optionValue(options, 'artifactSizer', 'artifact_sizer'));
    this.compression = options.compression ?? 'none';
    this.compressionThresholdBytes = options.compressionThresholdBytes;
    this.compact = options.compact === true || options.format === 'compact';
    this.livePublishEveryEntries = optionValue(options, 'livePublishEveryEntries', 'live_publish_every_entries');
    this.fieldNamePolicy = normalizeFieldNamePolicy(optionValue(options, 'fieldNamePolicy', 'field_name_policy'));
    this.fileMode = normalizeFileMode(optionValue(options, 'fileMode', 'file_mode'));
  }

  _configureRotation(options) {
    const rotationPolicy = optionValue(options, 'rotationPolicy', 'rotation_policy');
    this.maxEntries = policyNumber(rotationPolicy, options, ['maxEntries', 'max_entries'], ['maxEntries', 'max_entries'], 'rotation max entries', DEFAULT_MAX_ENTRIES);
    this.maxBytes = policyNumber(rotationPolicy, options, ['maxBytes', 'maxFileSize', 'max_file_size', 'max_bytes'], ['maxBytes', 'max_bytes'], 'rotation max file size', DEFAULT_MAX_BYTES);
    this.maxDurationUsec = policyUsec(rotationPolicy, options, ['maxDurationUsec', 'maxDuration', 'max_duration_usec', 'max_duration'], ['maxDurationUsec', 'max_duration_usec'], 'rotation max duration', DEFAULT_MAX_DURATION_USEC);
  }

  _configureRetention(options) {
    const retentionPolicy = optionValue(options, 'retentionPolicy', 'retention_policy');
    this.maxFiles = policyNumber(retentionPolicy, options, ['maxFiles', 'max_files'], ['maxFiles', 'max_files'], 'retention max files', DEFAULT_MAX_FILES);
    this.maxRetentionBytes = policyNumber(retentionPolicy, options, ['maxBytes', 'maxRetentionBytes', 'max_bytes', 'max_retention_bytes'], ['maxRetentionBytes', 'max_retention_bytes'], 'retention max bytes', DEFAULT_RETENTION_BYTES);
    this.maxRetentionAgeUsec = policyUsec(retentionPolicy, options, ['maxAgeUsec', 'maxRetentionAgeUsec', 'maxAge', 'max_age_usec', 'max_retention_age_usec', 'max_age'], ['maxRetentionAgeUsec', 'max_retention_age_usec'], 'retention max age', DEFAULT_RETENTION_AGE_USEC);
  }

  _deriveRotationFromRetention() {
    if (this.maxBytes === DEFAULT_MAX_BYTES && this.maxRetentionBytes > 0) {
      this.maxBytes = normalizeJournalMaxFileSize(Math.max(1, Math.floor(this.maxRetentionBytes / DERIVED_ROTATION_FRACTION)), this.compact);
    }
    if (this.maxDurationUsec === DEFAULT_MAX_DURATION_USEC && this.maxRetentionAgeUsec > 0n) {
      const fraction = BigInt(DERIVED_ROTATION_FRACTION);
      this.maxDurationUsec = (this.maxRetentionAgeUsec + fraction - 1n) / fraction;
      if (this.maxDurationUsec <= 0n) this.maxDurationUsec = 1n;
    }
  }

  _initializeRuntimeState(options) {
    this.activePath = null;
    this.writer = null;
    this.closed = false;
    this.openRetentionApplied = false;
    this._pathCounter = 0;
    this.lastRealtime = 0n;
    this.lastMonotonic = 0n;
    const headSeqnumOption = optionValue(options, 'headSeqnum', 'head_seqnum');
    this.nextSeqnum = headSeqnumOption ? BigInt(headSeqnumOption) : 1n;
  }

  _initializeIdentity(options) {
    const seqnumIdOption = optionValue(options, 'seqnumId', 'seqnum_id');
    const bootIdOption = optionValue(options, 'bootId', 'boot_id');
    const machineIdOption = optionValue(options, 'machineId', 'machine_id');
    if (this.identityMode === LOG_IDENTITY_STRICT) {
      if (machineIdOption === undefined || machineIdOption === null) throw new Error('strict identity requires machine id');
      if (bootIdOption === undefined || bootIdOption === null) throw new Error('strict identity requires boot id');
    }
    this.seqnumId = uuidOption(seqnumIdOption, 'seqnum id') || randomUUID();
    this.bootId = uuidOption(bootIdOption, 'boot id') || randomUUID();
    this.machineId = uuidOption(machineIdOption, 'machine id') || randomUUID();
    this.directory = join(this.rootDirectory, uuidToString(this.machineId));
  }

  _attachExistingChainState(options) {
    const chainState = this._scanChainState();
    this._applyChainSequenceState(chainState, options);
    this._applyChainTimestampState(chainState);
    this._attachChainActive(chainState);
  }

  _applyChainSequenceState(chainState, options) {
    const headSeqnumOption = optionValue(options, 'headSeqnum', 'head_seqnum');
    const seqnumIdOption = optionValue(options, 'seqnumId', 'seqnum_id');
    if (headSeqnumOption === undefined && chainState.tailSeqnum > 0n) this.nextSeqnum = chainState.tailSeqnum + 1n;
    if (seqnumIdOption === undefined && chainState.seqnumId) this.seqnumId = Buffer.from(chainState.seqnumId);
  }

  _applyChainTimestampState(chainState) {
    this.lastRealtime = chainState.tailRealtime;
    if (chainState.tailBootId && chainState.tailBootId.equals(this.bootId)) {
      this.lastMonotonic = chainState.tailMonotonic;
    }
  }

  _attachChainActive(chainState) {
    if (this.strictSystemdNaming && chainState.activePath) {
      this._archiveOnlineChainActive(chainState.activePath);
    }
    if (this.strictSystemdNaming && safeExistsSync(this._systemdActivePath())) {
      this._attachExistingActive(this._systemdActivePath());
    }
    if (!this.strictSystemdNaming) {
      if (chainState.activePath) this._attachExistingActive(chainState.activePath);
    }
  }

  _ensureDirectory() {
    safeMkdirSync(this.directory, { recursive: true });
  }

  _findOrCreateActiveFile() {
    if (this.strictSystemdNaming) this.activePath = this._systemdActivePath();
  }

  append(fields, options = {}) {
    if (this.closed) throw new Error('journal log is closed');
    if (fields.length === 0) throw new Error('empty entry');
    let appendOptions = this._entryOptionsForAppend(options);
    this._applyRetentionOnOpen();
    if (this.writer && this._shouldRotate(appendOptions.realtimeUsec)) {
      this._rotate(appendOptions);
    }
    if (!this.writer) {
      this._openWriter(appendOptions, LOG_LIFECYCLE_REASON_APPEND);
    }
    this._applyRetentionOnOpen();

    fields = prepareFieldsForPolicy(fields, this.fieldNamePolicy);

    const result = this.writer.append(this._fieldsForAppend(fields, appendOptions), appendOptions);
    this._captureWriterIdentity();
    return result;
  }

  appendRaw(payloads, options = {}) {
    if (this.closed) throw new Error('journal log is closed');
    payloads = prepareRawPayloadsForPolicy(payloads, this.fieldNamePolicy);
    const appendOptions = this._entryOptionsForAppend(options);
    this._applyRetentionOnOpen();
    if (this.writer && this._shouldRotate(appendOptions.realtimeUsec)) {
      this._rotate(appendOptions);
    }
    if (!this.writer) {
      this._openWriter(appendOptions, LOG_LIFECYCLE_REASON_APPEND);
    }
    this._applyRetentionOnOpen();

    const result = this.writer.appendRaw(this._payloadsForAppend(payloads, appendOptions), appendOptions);
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
    this._openWriter(options, LOG_LIFECYCLE_REASON_ROTATION);
    this._emitLifecycle({
      type: LOG_LIFECYCLE_ROTATED,
      reason: LOG_LIFECYCLE_REASON_ROTATION,
      archivedPath,
      activePath: this.activePath,
    });
    this._applyRetention(this.activePath);
  }

  _openWriter(options = {}, reason = LOG_LIFECYCLE_REASON_APPEND) {
    this._ensureActivePath(options);
    if (this._openExistingActive(options)) return;
    this._createActiveWriter(reason);
  }

  _ensureActivePath(options) {
    if (this.activePath) return;
    if (this.strictSystemdNaming) {
      this.activePath = this._systemdActivePath();
      return;
    }
    const headRealtime = optionUsec(options.realtimeUsec ?? options.realtime_usec, nowUsec());
    this.activePath = this._chainPathFor(this.seqnumId, this.nextSeqnum, headRealtime);
  }

  _openExistingActive(options) {
    for (let attempt = 0; attempt < 2; attempt++) {
      if (!this.activePath || !safeExistsSync(this.activePath)) return false;
      const opened = this._tryOpenExistingActive(this.activePath, true);
      if (opened) return true;
      this._ensureActivePath(options);
    }
    return false;
  }

  _tryOpenExistingActive(path, discardEmpty) {
    try {
      this.writer = Writer.open(path, this._openWriterOptions());
    } catch (error) {
      if (!isReplaceableActiveOpenError(error)) throw error;
      this._replaceActiveFile(path);
      return false;
    }
    if (discardEmpty && this.writer.header.n_entries === 0n) {
      this._discardEmptyOpenedWriter();
      return false;
    }
    this._captureWriterIdentity();
    return true;
  }

  _openWriterOptions() {
    return {
      livePublishEveryEntries: this.livePublishEveryEntries,
      fieldNamePolicy: writerPolicyForLogPolicy(this.fieldNamePolicy),
    };
  }

  _createActiveWriter(reason) {
    const opts = { headSeqnum: this.nextSeqnum, compression: this.compression, compact: this.compact };
    if (this.maxBytes > 0) opts.maxFileSize = this.maxBytes;
    if (this.compressionThresholdBytes !== undefined) {
      opts.compressionThresholdBytes = this.compressionThresholdBytes;
    }
    if (this.livePublishEveryEntries !== undefined) {
      opts.livePublishEveryEntries = this.livePublishEveryEntries;
    }
    opts.fileMode = this.fileMode;
    opts.fieldNamePolicy = writerPolicyForLogPolicy(this.fieldNamePolicy);
    if (this.seqnumId) opts.seqnumId = this.seqnumId;
    if (this.bootId) opts.bootId = this.bootId;
    if (this.machineId) opts.machineId = this.machineId;
    this.writer = Writer.create(this.activePath, opts);
    this._captureWriterIdentity();
    if (reason !== LOG_LIFECYCLE_REASON_ROTATION) {
      this._emitLifecycle({
        type: LOG_LIFECYCLE_CREATED,
        reason,
        activePath: this.activePath,
      });
    }
  }

  _discardEmptyOpenedWriter() {
    this.writer.close();
    unlinkIfExists(this.activePath);
    this.writer = null;
    if (!this.strictSystemdNaming) this.activePath = null;
  }

  _attachExistingActive(path) {
    this.activePath = path;
    this._tryOpenExistingActive(path, true);
  }

  _archiveOnlineChainActive(path) {
    let writer;
    try {
      writer = Writer.open(path);
    } catch (error) {
      if (!isReplaceableActiveOpenError(error)) throw error;
      this._replaceActiveFile(path);
      return;
    }
    if (writer.header.n_entries === 0n) {
      writer.close();
      unlinkIfExists(path);
      return;
    }
    writer.archiveTo(path);
  }

  _replaceActiveFile(path) {
    let header;
    try {
      header = readJournalHeader(path);
    } catch {
      this._disposeActiveFile(path);
      return;
    }

    const currentTail = this.nextSeqnum > 0n ? this.nextSeqnum - 1n : 0n;
    if (header.n_entries > 0n && header.tail_entry_seqnum >= currentTail) {
      this.seqnumId = Buffer.from(header.seqnum_id);
      this.nextSeqnum = header.tail_entry_seqnum + 1n;
      if (!isZeroUUID(header.tail_entry_boot_id)) this.bootId = Buffer.from(header.tail_entry_boot_id);
      this.lastRealtime = header.tail_entry_realtime;
      this.lastMonotonic = header.tail_entry_monotonic;
    }
    this._disposeActiveFile(path);
  }

  _disposeActiveFile(path) {
    let attempt = 0n;
    let target = disposedJournalPath(path, attempt);
    while (safeExistsSync(target)) {
      attempt += 1n;
      target = disposedJournalPath(path, attempt);
    }
    try {
      safeRenameSync(path, target);
    } catch (error) {
      if (error?.code === 'ENOENT') return;
      throw error;
    }
    syncDirectory(this.directory);
    if (this.activePath === path) this.activePath = null;
  }

  _captureWriterIdentity() {
    this.seqnumId = Buffer.from(this.writer.header.seqnum_id);
    this.bootId = Buffer.from(this.writer.bootId);
    this.machineId = Buffer.from(this.writer.header.machine_id);
    this.nextSeqnum = this.writer.nextSeqnum;
    this.lastRealtime = this.writer.header.tail_entry_realtime;
    this.lastMonotonic = this.writer.header.tail_entry_monotonic;
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
      tailRealtime: 0n,
      tailMonotonic: 0n,
      tailBootId: null,
    };
    for (const name of safeReaddirSync(this.directory)) {
      if (!parseArchivedJournalName(name, this.source)) continue;
      const path = join(this.directory, name);
      try {
        const header = readJournalHeader(path);
        if (header.tail_entry_seqnum > state.tailSeqnum) {
          state.tailSeqnum = header.tail_entry_seqnum;
          state.seqnumId = Buffer.from(header.seqnum_id);
          state.tailRealtime = header.tail_entry_realtime;
          state.tailMonotonic = header.tail_entry_monotonic;
          state.tailBootId = Buffer.from(header.tail_entry_boot_id);
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
      } catch {
        // Ignore unreadable archives while choosing a recoverable active file.
      }
    }
    return state;
  }

  _applyRetention(protectedPath = this.activePath) {
    const state = this._retentionState(protectedPath);
    this._deleteByMaxFiles(state);
    this._deleteByMaxBytes(state);
    this._deleteByMaxAge(state);
    syncDirectory(this.directory);
    if (state.deletedPaths.length > 0) {
      this._emitLifecycle({
        type: LOG_LIFECYCLE_DELETED,
        reason: LOG_LIFECYCLE_REASON_RETENTION,
        deletedPaths: state.deletedPaths,
      });
    }
  }

  _retentionState(activePath) {
    const archives = this._collectRetentionArchives();
    const state = { activePath, archives, totalBytes: 0, fileCount: archives.length, deletedPaths: [] };
    const activeInArchives = archives.some((archive) => activePath && archive.path === activePath);
    for (const archive of archives) state.totalBytes += archive.size;
    if (activePath && !activeInArchives) this._addActiveRetentionSize(state);
    return state;
  }

  _collectRetentionArchives() {
    const archives = [];
    for (const entry of safeReaddirSync(this.directory)) {
      const archive = this._retentionArchiveForEntry(entry);
      if (archive) archives.push(archive);
    }
    archives.sort(compareRetentionArchives);
    return archives;
  }

  _retentionArchiveForEntry(entry) {
    const parsed = parseArchivedJournalName(entry, this.source);
    if (!parsed) return null;
    const fullPath = join(this.directory, parsed.name);
    try {
      const stat = safeStatSync(fullPath);
      return {
        name: parsed.name,
        path: fullPath,
        size: this._retainedSize(fullPath, stat.size),
        headSeqnum: parsed.headSeqnum,
        headRealtime: parsed.headRealtime,
      };
    } catch {
      return null;
    }
  }

  _addActiveRetentionSize(state) {
    try {
      const stat = safeStatSync(state.activePath);
      state.totalBytes += this._retainedSize(state.activePath, stat.size);
      state.fileCount += 1;
    } catch {
      // Retention remains valid if the active file disappears between scans.
    }
  }

  _deleteByMaxFiles(state) {
    while (this.maxFiles > 0 && state.fileCount > this.maxFiles) {
      if (!this._deleteOldestRetainableArchive(state, true)) break;
    }
  }

  _deleteByMaxBytes(state) {
    while (this.maxRetentionBytes > 0 && state.totalBytes > this.maxRetentionBytes && state.archives.length > 0) {
      if (!this._deleteOldestRetainableArchive(state, false)) break;
    }
  }

  _deleteByMaxAge(state) {
    if (this.maxRetentionAgeUsec <= 0n) return;
    const cutoff = saturatingSubBigInt(nowUsec(), this.maxRetentionAgeUsec);
    while (state.archives.length > 0) {
      const index = state.archives.findIndex((archive) => archive.headRealtime <= cutoff && this._canDeleteArchive(archive, state.activePath));
      if (index === -1) break;
      this._deleteArchiveAt(state, index, false);
    }
  }

  _deleteOldestRetainableArchive(state, decrementCount) {
    const index = state.archives.findIndex((archive) => this._canDeleteArchive(archive, state.activePath));
    if (index === -1) return false;
    this._deleteArchiveAt(state, index, decrementCount);
    return true;
  }

  _deleteArchiveAt(state, index, decrementCount) {
    const [oldest] = state.archives.splice(index, 1);
    if (!unlinkIfExists(oldest.path)) return;
    state.totalBytes = Math.max(0, state.totalBytes - oldest.size);
    if (decrementCount) state.fileCount--;
    state.deletedPaths.push(oldest.path);
  }

  _canDeleteArchive(archive, activePath) {
    return !activePath || archive.path !== activePath;
  }

  enforceRetention() {
    if (this.closed) throw new Error('journal log is closed');
    this._applyRetention(this.activePath);
  }

  _applyRetentionOnOpen() {
    if (this.openRetentionApplied || !this.writer) return;
    this._applyRetention(this.activePath);
    this.openRetentionApplied = true;
  }

  _entryOptionsForAppend(options) {
    const appendOptions = { ...options };
    if (appendOptions.realtimeUsec === undefined && appendOptions.realtime_usec !== undefined) {
      appendOptions.realtimeUsec = appendOptions.realtime_usec;
    }
    if (appendOptions.monotonicUsec === undefined && appendOptions.monotonic_usec !== undefined) {
      appendOptions.monotonicUsec = appendOptions.monotonic_usec;
    }
    if (appendOptions.sourceRealtimeUsec === undefined && appendOptions.source_realtime_usec !== undefined) {
      appendOptions.sourceRealtimeUsec = appendOptions.source_realtime_usec;
    }
    if (appendOptions.realtimeUsec === undefined) {
      appendOptions.realtimeUsec = nowUsec();
    }
    appendOptions.realtimeUsec = BigInt(appendOptions.realtimeUsec);
    if (appendOptions.realtimeUsec <= this.lastRealtime) {
      appendOptions.realtimeUsec = this.lastRealtime + 1n;
    }
    if (appendOptions.monotonicUsec !== undefined) {
      appendOptions.monotonicUsec = BigInt(appendOptions.monotonicUsec);
      if (appendOptions.monotonicUsec <= this.lastMonotonic) {
        appendOptions.monotonicUsec = this.lastMonotonic + 1n;
      }
    }
    return appendOptions;
  }

  _fieldsForAppend(fields, options) {
    const withMetadata = [
      { name: '_BOOT_ID', value: Buffer.from(uuidToString(this._entryBootIdForAppend(options)), 'utf8') },
    ];
    const sourceRealtime = options.sourceRealtimeUsec;
    if (sourceRealtime !== undefined && sourceRealtime !== null && sourceRealtime !== 0 && sourceRealtime !== 0n) {
      withMetadata.push({ name: '_SOURCE_REALTIME_TIMESTAMP', value: Buffer.from(String(BigInt(sourceRealtime)), 'utf8') });
    }
    withMetadata.push(...fields);
    return withMetadata;
  }

  _payloadsForAppend(payloads, options) {
    const withMetadata = [
      Buffer.from(`_BOOT_ID=${uuidToString(this._entryBootIdForAppend(options))}`, 'utf8'),
    ];
    const sourceRealtime = options.sourceRealtimeUsec;
    if (sourceRealtime !== undefined && sourceRealtime !== null && sourceRealtime !== 0 && sourceRealtime !== 0n) {
      withMetadata.push(Buffer.from(`_SOURCE_REALTIME_TIMESTAMP=${BigInt(sourceRealtime)}`, 'utf8'));
    }
    withMetadata.push(...payloads);
    return withMetadata;
  }

  _entryBootIdForAppend(options) {
    const value = optionValue(options, 'bootId', 'boot_id');
    if (value === undefined || value === null) return this.bootId;
    const bootId = uuidOption(value, 'entry boot id');
    if (isZeroUUID(bootId)) return this.bootId;
    return bootId;
  }

  _retainedSize(path, fallback) {
    const journalSize = committedJournalSize(path, fallback);
    if (!this.artifactSizer) return journalSize;
    const artifactSize = this.artifactSizer(path);
    if (artifactSize === undefined || artifactSize === null) return journalSize;
    const value = Number(artifactSize);
    if (!Number.isFinite(value) || value < 0) throw new Error('artifact size must be a non-negative finite number');
    return journalSize + value;
  }

  _emitLifecycle(event) {
    if (!this.lifecycle) return;
    try {
      this.lifecycle(event);
    } catch (error) {
      if (typeof this.lifecycleErrorHandler === 'function') {
        this.lifecycleErrorHandler(error, event);
      }
    }
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

  activeFilePath() {
    return this.activePath || '';
  }

  activeJournalPath() {
    return this.activeFilePath();
  }

  journalDirectory() {
    return this.directory;
  }

  configuredDirectory() {
    return this.rootDirectory;
  }

  machineID() {
    return Buffer.from(this.machineId);
  }

  bootID() {
    return this.bootId ? Buffer.from(this.bootId) : null;
  }

  sourceName() {
    return this.source;
  }
}

export default Log;

function validateJournalSource(source) {
  if (source === '' || source === '.' || source === '..') throw new Error('invalid journal source');
  for (let i = 0; i < source.length; i++) {
    if (!isJournalSourceCodePoint(source.charCodeAt(i))) throw new Error('invalid journal source');
  }
}

function isJournalSourceCodePoint(c) {
  return isLowerAsciiLetter(c) || isUpperAsciiLetter(c) || isAsciiDigit(c) ||
    c === 0x5f || c === 0x2d || c === 0x2e;
}

function isLowerAsciiLetter(c) {
  return c >= 0x61 && c <= 0x7a;
}

function isUpperAsciiLetter(c) {
  return c >= 0x41 && c <= 0x5a;
}

function isAsciiDigit(c) {
  return c >= 0x30 && c <= 0x39;
}

function readJournalHeader(path) {
  const fd = safeOpenSync(path, 'r');
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
  const fd = safeOpenSync(path, 'r');
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
    safeUnlinkSync(path);
    return true;
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error;
    return false;
  }
}

function isReplaceableActiveOpenError(error) {
  return String(error?.message ?? error).includes('unsupported journal');
}

function disposedJournalPath(path, attempt) {
  const stem = path.endsWith('.journal') ? path.slice(0, -'.journal'.length) : path;
  const stamp = process.hrtime.bigint() & 0xffffffffffffffffn;
  const suffix = BigInt(process.pid) ^ BigInt(attempt);
  return `${stem}@${stamp.toString(16).padStart(16, '0')}-${suffix.toString(16).padStart(16, '0')}.journal~`;
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

function compareRetentionArchives(a, b) {
  if (a.headRealtime !== b.headRealtime) return a.headRealtime < b.headRealtime ? -1 : 1;
  if (a.headSeqnum !== b.headSeqnum) return a.headSeqnum < b.headSeqnum ? -1 : 1;
  return a.path.localeCompare(b.path);
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

function optionValue(object, ...names) {
  for (const name of names) {
    if (Object.prototype.hasOwnProperty.call(object, name)) return Reflect.get(object, name);
  }
  return undefined;
}

function normalizeOpenMode(options) {
  let value = optionValue(options, 'openMode', 'open_mode');
  if (value === undefined && (options.eagerOpen === true || options.eager_open === true)) value = LOG_OPEN_EAGER;
  if (value === undefined || value === null || value === '') return LOG_OPEN_LAZY;
  value = String(value).toLowerCase();
  if (value === LOG_OPEN_LAZY || value === LOG_OPEN_EAGER) return value;
  throw new Error(`unsupported log open mode: ${value}`);
}

function normalizeIdentityMode(options) {
  let value = optionValue(options, 'identityMode', 'identity_mode');
  if (value === undefined || value === null || value === '') return LOG_IDENTITY_AUTO;
  value = String(value).toLowerCase();
  if (value === LOG_IDENTITY_AUTO || value === LOG_IDENTITY_STRICT) return value;
  throw new Error(`unsupported log identity mode: ${value}`);
}

function uuidOption(value, label) {
  if (value === undefined || value === null) return null;
  let out;
  if (typeof value === 'string') {
    const clean = value.trim().replaceAll('-', '');
    if (!/^[0-9a-fA-F]{32}$/.test(clean)) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
    out = stringToUUID(clean);
  } else {
    out = Buffer.from(value);
  }
  if (out.length !== 16) throw new Error(`${label} must be 16 bytes or 32 hex characters`);
  return out;
}

function positiveOptionalNumber(value, label, fallback) {
  if (value === undefined || value === null) return fallback;
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) throw new Error(`${label} must be greater than 0`);
  return number;
}

function positiveOptionalUsec(value, label, fallback) {
  if (value === undefined || value === null) return BigInt(fallback);
  const usec = BigInt(value);
  if (usec <= 0n) throw new Error(`${label} must be greater than 0`);
  return usec;
}

function policyNumber(policy, options, policyNames, optionNames, label, fallback) {
  if (policy) return positiveOptionalNumber(optionValue(policy, ...policyNames), label, fallback);
  return optionValue(options, ...optionNames) ?? fallback;
}

function policyUsec(policy, options, policyNames, optionNames, label, fallback) {
  if (policy) return positiveOptionalUsec(optionValue(policy, ...policyNames), label, fallback);
  return optionUsec(optionValue(options, ...optionNames), fallback);
}

function normalizeLifecycle(value) {
  if (value === undefined || value === null) return null;
  if (typeof value === 'function') return value;
  if (typeof value.onLogLifecycleEvent === 'function') return (event) => value.onLogLifecycleEvent(event);
  if (typeof value.onLifecycleEvent === 'function') return (event) => value.onLifecycleEvent(event);
  throw new Error('lifecycle must be a function or observer object');
}

function normalizeArtifactSizer(value) {
  if (value === undefined || value === null) return null;
  if (typeof value === 'function') return value;
  if (typeof value.journalArtifactSize === 'function') return (path) => value.journalArtifactSize(path);
  if (typeof value.JournalArtifactSize === 'function') return (path) => value.JournalArtifactSize(path);
  throw new Error('artifact sizer must be a function or provider object');
}

function saturatingSubBigInt(value, amount) {
  return value >= amount ? value - amount : 0n;
}

function syncDirectory(path) {
  if (process.platform === 'win32') return false;
  const fd = safeOpenSync(path, 'r');
  try {
    fsyncSync(fd);
    return true;
  } finally {
    closeSync(fd);
  }
}
