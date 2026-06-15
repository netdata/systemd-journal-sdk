// TypeScript type definitions for @netdata/systemd-journal-sdk
// Hand-written, maintained alongside src/index.js.

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/** Node.js Buffer or Uint8Array — the SDK accepts and returns both. */
type Buffer = Uint8Array;

declare module "@netdata/systemd-journal-sdk" {
  /** A Buffer-like byte sequence – either a Node Buffer or Uint8Array. */
  type Bytes = Buffer | Uint8Array;

  /** A journal field: a name (UTF-8 string or raw bytes) and value (bytes). */
  interface Field {
    name: string | Bytes;
    value: Bytes;
  }

  /** Parsed cursor components. */
  interface CursorParts {
    seqnumId: string;
    bootId: string;
    realtime: bigint;
    seqnum: bigint;
  }

  /** Result of getSeqnum(). */
  interface SeqnumResult {
    seqnum: bigint | null;
    seqnum_id: string;
  }

  /** Result of getMonotonicUsec(). */
  interface MonotonicUsecResult {
    monotonic: bigint | null;
    boot_id: Bytes | null;
  }

  /** A decoded journal entry. */
  interface JournalEntry {
    cursor: string;
    realtime: bigint | null;
    monotonic: bigint | null;
    seqnum: bigint | null;
    boot_id: Bytes | null;
    fields: Record<string, Bytes>;
    fieldValues: Record<string, Bytes[]>;
    rawFields: [Bytes, Bytes][] | null;
    rawFieldValues: Map<string, Bytes[]> | null;
    payloads: Bytes[] | null;
  }

  /** A boot info entry. */
  interface BootInfo {
    boot_id: Bytes;
    first_realtime: bigint;
    last_realtime: bigint;
  }

  type ReaderAccessMode =
    | typeof READER_ACCESS_AUTO
    | typeof READER_ACCESS_READ_AT;

  type ReaderBoundsMode =
    | typeof READER_BOUNDS_LIVE
    | typeof READER_BOUNDS_SNAPSHOT;

  interface ReaderOptions {
    accessMode?: ReaderAccessMode | "auto" | "read-at" | "readat" | "pread" | "buffer";
    mmapStrategy?: ReaderAccessMode | "auto" | "read-at" | "readat" | "pread" | "buffer";
    bounds?: ReaderBoundsMode | "live" | "snapshot";
    windowSizeBytes?: number;
    windowSize?: number;
    maxWindows?: number;
    maxRowArenaBytes?: number;
    rowArenaSegmentBytes?: number;
    zstdTimeoutMs?: number;
  }

  interface ReaderAccessStats {
    requestedAccessMode: string;
    selectedAccessMode: string;
    selectedBackend: string;
    fallbackReason: string;
    bounds: string;
    visibleSize: number;
    windowSizeBytes: number;
    maxWindows: number;
    windowsCreated: number;
    windowHits: number;
    windowMisses: number;
    evictions: number;
    pinnedWindows: number;
    readBufferBytes: number;
    rowArenaPeakBytes: number;
    tempCopyBytes: number;
    tempCopyCount: number;
    shortReads: number;
    readSyncUsesPosition: boolean;
  }

  const READER_ACCESS_AUTO: "auto";
  const READER_ACCESS_READ_AT: "read-at";
  const READER_BOUNDS_LIVE: "live";
  const READER_BOUNDS_SNAPSHOT: "snapshot";
  const DEFAULT_WINDOW_SIZE_BYTES: number;
  const DEFAULT_MAX_WINDOWS: number;
  const DEFAULT_MAX_ROW_ARENA_BYTES: number;
  const DEFAULT_ROW_ARENA_SEGMENT_BYTES: number;

  class UnsupportedAccessModeError extends Error {
    accessMode: string;
  }

  /**
   * Cooperating match filter used by the readers. Repeated `addMatch`
   * calls for the same field are OR alternatives; `addDisjunction` /
   * `addConjunction` group them like the libsystemd match API.
   */
  class FilterBuilder {
    constructor();
    addMatch(data: string | Bytes): void;
    addDisjunction(): void;
    addConjunction(): void;
    matches(entry: JournalEntry): boolean;
  }

  // -----------------------------------------------------------------------
  // Reader
  // -----------------------------------------------------------------------

  class FileReader {
    static open(path: string, options?: ReaderOptions): FileReader;
    close(): void;
    readonly path: string | null;
    readonly header: FileHeader;

    addMatch(data: string | Bytes): void;
    addDisjunction(): void;
    addConjunction(): void;
    flushMatches(): void;

    seekHead(): void;
    seekTail(): void;
    seekRealtimeUsec(usec: bigint | number): void;

    step(): boolean;
    stepBack(): boolean;

    getEntry(): JournalEntry;
    getCursor(): string;
    testCursor(cursor: string): boolean;
    getRealtimeUsec(): bigint | null;

    entryDataRestart(): void;
    enumerateEntryPayload(): Bytes | null;
    getEntryPayload(fieldName: string | Bytes): Bytes | null;

    getRaw(fieldName: Bytes): Bytes | null;
    getRawValues(fieldName: Bytes): Bytes[] | null;
    visitEntryPayloads(visitor: (payload: Bytes) => void): void;

    enumerateFields(): Set<string>;
    queryUnique(fieldName: string | Bytes): string[];

    explore(query: ExplorerQuery): ExplorerResult;
    exploreWithStrategy(query: ExplorerQuery, strategy: ExplorerStrategy): ExplorerResult;
    exploreWithStrategyAndControl(query: ExplorerQuery, strategy: ExplorerStrategy, control: ExplorerControl): ExplorerResult;
    accessStats(): ReaderAccessStats;
  }

  class DirectoryReader {
    static open(path: string, options?: ReaderOptions): DirectoryReader;
    static openFiles(paths: string[], options?: ReaderOptions): DirectoryReader;
    close(): void;

    addMatch(data: string | Bytes): void;
    addDisjunction(): void;
    addConjunction(): void;
    flushMatches(): void;

    seekHead(): void;
    seekTail(): void;
    seekRealtimeUsec(usec: bigint | number): void;

    step(): boolean;
    stepBack(): boolean;

    getEntry(): JournalEntry;
    getCursor(): string;
    testCursor(cursor: string): boolean;
    getRealtimeUsec(): bigint | null;

    entryDataRestart(): void;
    enumerateEntryPayload(): Bytes | null;
    getEntryPayload(fieldName: string | Bytes): Bytes | null;

    getRaw(fieldName: Bytes): Bytes | null;
    getRawValues(fieldName: Bytes): Bytes[] | null;
    visitEntryPayloads(visitor: (payload: Bytes) => void): void;

    enumerateFields(): Set<string>;
    queryUnique(fieldName: string | Bytes): string[];

    listBoots(): BootInfo[];
  }

  // -----------------------------------------------------------------------
  // Header parsing
  // -----------------------------------------------------------------------

  // Field names and types match the snake_case keys returned by
  // parseFileHeader at runtime (verified against a live header object).
  interface FileHeader {
    signature: string;
    compatible_flags: number;
    incompatible_flags: number;
    state: number;
    file_id: Bytes;
    machine_id: Bytes;
    tail_entry_boot_id: Bytes;
    seqnum_id: Bytes;
    header_size: bigint;
    arena_size: bigint;
    data_hash_table_offset: bigint;
    data_hash_table_size: bigint;
    field_hash_table_offset: bigint;
    field_hash_table_size: bigint;
    tail_object_offset: bigint;
    n_objects: bigint;
    n_entries: bigint;
    tail_entry_seqnum: bigint;
    head_entry_seqnum: bigint;
    entry_array_offset: bigint;
    head_entry_realtime: bigint;
    tail_entry_realtime: bigint;
    tail_entry_monotonic: bigint;
    n_data: bigint;
    n_fields: bigint;
    n_tags: bigint;
    n_entry_arrays: bigint;
    data_hash_chain_depth: bigint;
    field_hash_chain_depth: bigint;
    tail_entry_array_offset: number;
    tail_entry_array_n_entries: number;
    tail_entry_offset: bigint;
  }

  interface ObjectHeader {
    type: number;
    flags: number;
    size: bigint;
    hash: bigint;
    nextHashOffset: bigint;
    nextFieldOffset: bigint;
    payloadOffset: bigint | null;
    payloadHash: bigint | null;
    entryOffset: bigint | null;
    nEntries: bigint | null;
  }

  const HEADER_SIZE: number;
  const OBJECT_HEADER_SIZE: number;
  const ENTRY_OBJECT_HEADER_SIZE: number;
  const DATA_OBJECT_HEADER_SIZE: number;
  const FIELD_OBJECT_HEADER_SIZE: number;
  const HASH_ITEM_SIZE: number;

  const OBJECT_TYPE_DATA: number;
  const OBJECT_TYPE_FIELD: number;
  const OBJECT_TYPE_ENTRY: number;
  const OBJECT_TYPE_DATA_HASH_TABLE: number;
  const OBJECT_TYPE_FIELD_HASH_TABLE: number;
  const OBJECT_TYPE_ENTRY_ARRAY: number;

  function parseFileHeader(buf: Bytes): FileHeader;
  function parseObjectHeader(buf: Bytes, offset?: number): ObjectHeader;

  // -----------------------------------------------------------------------
  // Entry / DATA parsing
  // -----------------------------------------------------------------------

  function parseEntryObject(buf: Bytes, offset: number, compact?: boolean): object;
  function parseDataObject(buf: Bytes, offset: number, compact?: boolean): object;
  function parseDataPayload(buf: Bytes, offset: number, compact?: boolean): object;

  // -----------------------------------------------------------------------
  // Binary helpers
  // -----------------------------------------------------------------------

  function readUint64LE(buf: Bytes, offset?: number): bigint;
  function writeUint64LE(buf: Bytes, offset: number, value: bigint | number): void;
  function writeUint32LE(buf: Bytes, offset: number, value: number): void;
  function writeUint8(buf: Bytes, offset: number, value: number): void;
  function align8(value: number | bigint): bigint;
  function bufEqual(a: Bytes, b: Bytes): boolean;
  function uuidToString(uuid: Bytes): string;
  function stringToUUID(hex: string): Bytes;
  function isZeroUUID(uuid: Bytes): boolean;
  function randomUUID(): Bytes;

  // -----------------------------------------------------------------------
  // Hash helpers
  // -----------------------------------------------------------------------

  function sipHash24(key: Bytes, msg: Bytes): bigint;
  function jenkinsHash64(data: Bytes): bigint;
  function parseMatchString(s: string): { field: Bytes; value: Bytes };

  // -----------------------------------------------------------------------
  // Compression helpers
  // -----------------------------------------------------------------------

  function decompressZstSync(input: Bytes): Bytes;
  function isJournalFileName(name: string): boolean;
  function isZstFile(path: string): boolean;

  // -----------------------------------------------------------------------
  // Writer
  // -----------------------------------------------------------------------

  interface WriterOptions {
    compact?: boolean | string;
    format?: string;
    compression?: string;
    machineId?: Bytes;
    bootId?: Bytes;
    seqnumId?: Bytes;
    fileMode?: number;
    maxFileSize?: number;
    livePublishEveryEntries?: number;
    live_publish_every_entries?: number;
    sealOptions?: SealOptions;
    seal_options?: SealOptions;
    fieldNamePolicy?: FieldNamePolicy;
    field_name_policy?: FieldNamePolicy;
  }

  type FieldNamePolicy =
    | typeof FIELD_NAME_POLICY_JOURNALD
    | typeof FIELD_NAME_POLICY_JOURNAL_APP
    | typeof FIELD_NAME_POLICY_RAW;

  const FIELD_NAME_POLICY_JOURNALD: unique symbol;
  const FIELD_NAME_POLICY_JOURNAL_APP: unique symbol;
  const FIELD_NAME_POLICY_RAW: unique symbol;

  const COMPRESSION_NONE: number;
  const COMPRESSION_ZSTD: number;
  const COMPRESSION_XZ: number;
  const COMPRESSION_LZ4: number;
  const DEFAULT_JOURNAL_FILE_MODE: number;
  const DEFAULT_COMPRESS_THRESHOLD: number;
  const MIN_COMPRESS_THRESHOLD: number;

  class SealOptions {
    constructor(opts?: { secpar?: number; seedLen?: number });
    secpar: number;
    seedLen: number;
  }

  class SealState {
    constructor(secpar?: number, seedLen?: number);
  }

  class Writer {
    static create(path: string, options?: WriterOptions): Writer;
    readonly path: string;
    readonly closed: boolean;

    append(fields: Field[], options?: {
      realtimeUsec?: bigint | number;
      monotonicUsec?: bigint | number;
      sourceRealtimeUsec?: bigint | number;
      source_realtime_usec?: bigint | number;
    }): void;
    appendRaw(payloads: Bytes[], options?: {
      realtimeUsec?: bigint | number;
      monotonicUsec?: bigint | number;
      sourceRealtimeUsec?: bigint | number;
      source_realtime_usec?: bigint | number;
    }): void;
    sync(): void;
    close(): void;
    closeOffline(): void;
    archiveTo(path: string): void;
  }

  // -----------------------------------------------------------------------
  // Directory Writer (Log)
  // -----------------------------------------------------------------------

  const LOG_OPEN_LAZY: "lazy";
  const LOG_OPEN_EAGER: "eager";
  const LOG_IDENTITY_AUTO: "auto";
  const LOG_IDENTITY_STRICT: "strict";

  const LOG_LIFECYCLE_CREATED: "created";
  const LOG_LIFECYCLE_ROTATED: "rotated";
  const LOG_LIFECYCLE_DELETED: "deleted";
  const LOG_LIFECYCLE_REASON_APPEND: "append";
  const LOG_LIFECYCLE_REASON_EAGER_OPEN: "eager_open";
  const LOG_LIFECYCLE_REASON_ROTATION: "rotation";
  const LOG_LIFECYCLE_REASON_RETENTION: "retention";

  interface LogLifecycleEvent {
    type: string;
    reason?: string;
    path?: string;
  }

  interface LogOptions {
    source?: string;
    machineId?: Bytes;
    bootId?: Bytes;
    maxEntries?: number;
    maxBytes?: number;
    maxDurationUsec?: bigint | number;
    maxFiles?: number;
    maxRetentionBytes?: number;
    maxRetentionAgeUsec?: bigint | number;
    fileMode?: number;
    identityMode?: "auto" | "strict";
    openMode?: "lazy" | "eager";
    strictSystemdNaming?: boolean;
    livePublishEveryEntries?: number;
    live_publish_every_entries?: number;
    fieldNamePolicy?: FieldNamePolicy;
    field_name_policy?: FieldNamePolicy;
    rotationPolicy?: RotationPolicy;
    retentionPolicy?: RetentionPolicy;
    lifecycle?: (event: LogLifecycleEvent) => void;
    artifactSizer?: (entry: LogLifecycleEvent) => number;
  }

  interface RotationPolicy {
    maxEntries?: number;
    maxBytes?: number;
    maxDurationUsec?: bigint | number;
    maxFiles?: number;
  }

  interface RetentionPolicy {
    maxFiles?: number;
    maxBytes?: number;
    maxAgeUsec?: bigint | number;
  }

  class Log {
    constructor(directory: string, options?: LogOptions);
    readonly closed: boolean;
    readonly writer: Writer | null;

    append(fields: Field[], options?: {
      sourceRealtimeUsec?: bigint | number;
      source_realtime_usec?: bigint | number;
    }): void;
    appendRaw(payloads: Bytes[], options?: {
      sourceRealtimeUsec?: bigint | number;
      source_realtime_usec?: bigint | number;
    }): void;
    sync(): void;
    close(): void;
    activeFile(): string;
    activeFilePath(): string;
    configuredDirectory(): string;
    journalDirectory(): string;
    machineID(): Bytes;
    bootID(): Bytes;
    sourceName(): string;
    enforceRetention(): void;
  }

  // -----------------------------------------------------------------------
  // Explorer
  // -----------------------------------------------------------------------

  const Direction: { readonly Forward: 0; readonly Backward: 1 };
  type Direction = (typeof Direction)[keyof typeof Direction];

  const ExplorerAnchorKind: {
    readonly Auto: "auto";
    readonly Head: "head";
    readonly Tail: "tail";
    readonly Realtime: "realtime";
  };
  type ExplorerAnchorKind = (typeof ExplorerAnchorKind)[keyof typeof ExplorerAnchorKind];

  class ExplorerAnchor {
    constructor(kind?: ExplorerAnchorKind, realtimeUsec?: bigint | number);
    kind: ExplorerAnchorKind;
    realtimeUsec: bigint;
    static auto(): ExplorerAnchor;
    static head(): ExplorerAnchor;
    static tail(): ExplorerAnchor;
    static realtime(usec: bigint | number): ExplorerAnchor;
  }

  const ExplorerFieldMode: {
    readonly AllValues: "all_values";
    readonly FirstValue: "first_value";
  };
  type ExplorerFieldMode = (typeof ExplorerFieldMode)[keyof typeof ExplorerFieldMode];

  const ExplorerStrategy: {
    readonly Traversal: "traversal";
    readonly Index: "index";
    readonly Compare: "compare";
  };
  type ExplorerStrategy = (typeof ExplorerStrategy)[keyof typeof ExplorerStrategy];

  const ExplorerStopReason: {
    readonly TimedOut: "timed_out";
    readonly Cancelled: "cancelled";
  };
  type ExplorerStopReason = (typeof ExplorerStopReason)[keyof typeof ExplorerStopReason];

  class ExplorerError extends Error {}
  class ExplorerUnsupported extends ExplorerError {}

  class ExplorerFilter {
    constructor(field: string | Bytes, values?: (string | Bytes)[]);
    field: Bytes;
    values: Bytes[];
    static new(field: string | Bytes, values?: (string | Bytes)[]): ExplorerFilter;
  }

  class ExplorerFtsPattern {
    constructor(parts?: Bytes[], negative?: boolean);
    parts: Bytes[];
    negative: boolean;
    static substring(pattern: string | Bytes, negative?: boolean): ExplorerFtsPattern;
    matches(value: string | Bytes): boolean;
  }

  class ExplorerSampling {
    constructor(init?: {
      budget?: bigint | number;
      matchedFiles?: bigint | number;
      matched_files?: bigint | number;
      fileHeadRealtimeUsec?: bigint | number;
      file_head_realtime_usec?: bigint | number;
      fileTailRealtimeUsec?: bigint | number;
      file_tail_realtime_usec?: bigint | number;
      fileHeadSeqnum?: bigint | number;
      file_head_seqnum?: bigint | number;
      fileTailSeqnum?: bigint | number;
      file_tail_seqnum?: bigint | number;
      fileEntries?: bigint | number;
      file_entries?: bigint | number;
    });
    budget: bigint;
    matchedFiles: bigint;
    fileHeadRealtimeUsec: bigint;
    fileTailRealtimeUsec: bigint;
    fileHeadSeqnum: bigint;
    fileTailSeqnum: bigint;
    fileEntries: bigint;
  }

  class ExplorerStats {
    constructor();
    rowsExamined: bigint;
    rowsMatched: bigint;
    facetRowsMatched: bigint;
    rowsReturned: bigint;
    rowsUnsampled: bigint;
    rowsEstimated: bigint;
    samplingSampled: bigint;
    samplingUnsampled: bigint;
    samplingEstimated: bigint;
    lastRealtimeUsec: bigint;
    maxSourceRealtimeDeltaUsec: bigint;
    dataRefsSeen: bigint;
    dataRefsSkipped: bigint;
    dataPayloadsLoaded: bigint;
    dataObjectsClassified: bigint;
    dataCacheHits: bigint;
    dataCacheMisses: bigint;
    payloadsDecompressed: bigint;
    ftsScans: bigint;
    facetUpdates: bigint;
    histogramUpdates: bigint;
    returnedRowExpansions: bigint;
    earlyStopOpportunities: bigint;
    earlyStops: bigint;
    copy(): ExplorerStats;
    toJson(): Record<string, number>;
  }

  class ExplorerRow {
    constructor(realtimeUsec: bigint | number, cursor: string, payloads?: Bytes[]);
    realtimeUsec: bigint;
    cursor: string;
    payloads: Bytes[];
  }

  class ExplorerHistogramBucket {
    constructor(startUsec: bigint | number, endUsec: bigint | number);
    startRealtimeUsec: bigint;
    endRealtimeUsec: bigint;
    values: Map<string, bigint>;
  }

  class ExplorerHistogram {
    constructor(field?: string | Bytes, buckets?: ExplorerHistogramBucket[]);
    field: Bytes;
    buckets: ExplorerHistogramBucket[];
  }

  class ExplorerComparison {
    constructor();
    traversalDuration: number;
    indexDuration: number;
    traversalStats: ExplorerStats;
    indexStats: ExplorerStats;
  }

  class ExplorerResult {
    constructor();
    rows: ExplorerRow[];
    facets: Map<string, Map<string, bigint>>;
    histogram: ExplorerHistogram | null;
    columnFields: Set<string>;
    stats: ExplorerStats;
    comparison: ExplorerComparison | null;
  }

  class ExplorerProgress {
    constructor(stats: ExplorerStats, elapsed: number);
    stats: ExplorerStats;
    elapsed: number;
  }

  class ExplorerQuery {
    constructor();
    afterRealtimeUsec: bigint | null;
    beforeRealtimeUsec: bigint | null;
    anchor: ExplorerAnchor;
    direction: Direction;
    limit: number;
    filters: ExplorerFilter[];
    facets: Bytes[];
    histogram: Bytes | null;
    histogramAfterRealtimeUsec: bigint | null;
    histogramBeforeRealtimeUsec: bigint | null;
    histogramTargetBuckets: number;
    ftsTerms: ExplorerFtsPattern[];
    ftsPatterns: Bytes[];
    ftsNegativePatterns: Bytes[];
    fieldMode: ExplorerFieldMode;
    excludeFacetFieldFilters: boolean;
    useSourceRealtime: boolean;
    realtimeSlackUsec: bigint;
    stopWhenRowsFull: boolean;
    stopWhenRowsFullCheckEvery: number;
    sampling: ExplorerSampling | null;
    debugCollectColumnFieldsByRowTraversal: boolean;

    withFilter(field: string | Bytes, values: (string | Bytes)[]): this;
    withFacet(field: string | Bytes): this;
    withHistogram(field: string | Bytes): this;
    withFtsPattern(pattern: string | Bytes): this;
    withFtsNegativePattern(pattern: string | Bytes): this;
  }

  class ExplorerControl {
    constructor();
    deadline: number | null;
    cancellation: (() => boolean) | null;
    progress: ((p: ExplorerProgress) => void) | null;
    matchedRow: ((realtimeUsec: bigint, rowsMatched: bigint) => boolean | void) | null;
    progressIntervalMs: number;
    stopReason: ExplorerStopReason | null;

    setDeadline(deadline: number): void;
    setCancellationCallback(cb: () => boolean): void;
    setProgressCallback(cb: (p: ExplorerProgress) => void): void;
    setMatchedRowCallback(cb: (realtimeUsec: bigint, rowsMatched: bigint) => boolean | void): void;
    setProgressIntervalMs(ms: number): void;
  }

  const UNSET_VALUE: Buffer;
  const DEFAULT_HISTOGRAM_TARGET_BUCKETS: number;
  const DEFAULT_TIME_SLACK_USEC: bigint;
  const EXPLORER_CONTROL_CHECK_EVERY_ROWS: bigint;
  const EXPLORER_PROGRESS_INTERVAL_MS: number;

  // -----------------------------------------------------------------------
  // SdJournal facade
  // -----------------------------------------------------------------------

  const OUTPUT_MODE_DEFAULT: "default";
  const OUTPUT_MODE_JSON: "json";
  const OUTPUT_MODE_EXPORT: "export";

  type OutputMode =
    | typeof OUTPUT_MODE_DEFAULT
    | typeof OUTPUT_MODE_JSON
    | typeof OUTPUT_MODE_EXPORT;

  /** Visitor callback for SdJournalVisitUniqueValues. Must return
   * a falsy value on success or throw to abort enumeration (matching
   * the Rust Result::Err visitor semantics). */
  type UniqueValueVisitor = (value: Bytes) => void;

  class SdJournal {
    static open(path: string, options?: ReaderOptions): SdJournal;
    static openFile(path: string, options?: ReaderOptions): SdJournal;
    static openDirectory(path: string, options?: ReaderOptions): SdJournal;
    static openFiles(paths: string[], options?: ReaderOptions): SdJournal;

    reader: FileReader | DirectoryReader;
    outputMode: OutputMode;

    close(): void;
    addMatch(data: string | Bytes): void;
    addDisjunction(): void;
    addConjunction(): void;
    flushMatches(): void;
    seekHead(): void;
    seekTail(): void;
    seekRealtimeUsec(usec: bigint | number): void;
    seekCursor(cursor: string): void;
    setOutputMode(mode: OutputMode): void;

    next(): number;
    previous(): number;

    getEntry(): JournalEntry;
    getCursor(): string;
    testCursor(cursor: string): boolean;
    getRealtimeUsec(): bigint | null;
    getSeqnum(): SeqnumResult;
    getMonotonicUsec(): MonotonicUsecResult;

    restartData(): void;
    enumerateAvailableData(): Bytes | null;
    getData(fieldName: string | Bytes): Bytes | null;

    processOutput(entry: JournalEntry): Bytes | string;

    listBoots(): BootInfo[];

    enumerateFields(): string[];
    restartFields(): void;
    enumerateField(): string | null;

    queryUnique(fieldName: string | Bytes): [string, Bytes][];
    visitUniqueValues(fieldName: string | Bytes, visitor: UniqueValueVisitor): null;
    queryUniqueState(fieldName: string | Bytes): void;
    restartUnique(): void;
    enumerateAvailableUnique(): Bytes | null;
  }

  function SdJournalOpen(path: string, flags: number, options?: ReaderOptions): SdJournal;
  function SdJournalOpenFile(path: string, flags: number, options?: ReaderOptions): SdJournal;
  function SdJournalOpenDirectory(path: string, flags: number, options?: ReaderOptions): SdJournal;
  function SdJournalOpenFiles(paths: string[], flags: number, options?: ReaderOptions): SdJournal;
  function SdJournalClose(journal: SdJournal): void;

  function SdJournalAddMatch(journal: SdJournal, data: string | Bytes): void;
  function SdJournalAddDisjunction(journal: SdJournal): void;
  function SdJournalAddConjunction(journal: SdJournal): void;
  function SdJournalFlushMatches(journal: SdJournal): void;
  function SdJournalNext(journal: SdJournal): number;
  function SdJournalNextSkip(journal: SdJournal, skip: number): number;
  function SdJournalPrevious(journal: SdJournal): number;
  function SdJournalPreviousSkip(journal: SdJournal, skip: number): number;

  function SdJournalSeekHead(journal: SdJournal): void;
  function SdJournalSeekTail(journal: SdJournal): void;
  function SdJournalSeekRealtimeUsec(journal: SdJournal, usec: bigint | number): void;
  function SdJournalSeekCursor(journal: SdJournal, cursor: string): void;

  function SdJournalGetEntry(journal: SdJournal): JournalEntry;
  function SdJournalGetData(journal: SdJournal, fieldName: string | Bytes): Bytes | null;
  function SdJournalRestartData(journal: SdJournal): void;
  function SdJournalEnumerateAvailableData(journal: SdJournal): Bytes | null;
  function SdJournalGetRealtimeUsec(journal: SdJournal): bigint | null;
  function SdJournalGetSeqnum(journal: SdJournal): SeqnumResult;
  function SdJournalGetMonotonicUsec(journal: SdJournal): MonotonicUsecResult;
  function SdJournalGetCursor(journal: SdJournal): string;
  function SdJournalTestCursor(journal: SdJournal, cursor: string): boolean;

  function SdJournalEnumerateFields(journal: SdJournal): string[];
  function SdJournalRestartFields(journal: SdJournal): void;
  function SdJournalEnumerateField(journal: SdJournal): string | null;

  function SdJournalQueryUnique(journal: SdJournal, fieldName: string | Bytes): [string, Bytes][];
  function SdJournalVisitUniqueValues(journal: SdJournal, fieldName: string | Bytes, visitor: UniqueValueVisitor): null;
  function SdJournalQueryUniqueState(journal: SdJournal, fieldName: string | Bytes): void;
  function SdJournalRestartUnique(journal: SdJournal): void;
  function SdJournalEnumerateAvailableUnique(journal: SdJournal): Bytes | null;

  function SdJournalListBoots(journal: SdJournal): BootInfo[];
  function SdJournalSetOutputMode(journal: SdJournal, mode: OutputMode): void;
  function SdJournalProcessOutput(journal: SdJournal, entry: JournalEntry): Bytes | string;

  // Output helpers
  function exportEntryBuffer(entry: JournalEntry): Bytes;
  function exportEntry(entry: JournalEntry): Bytes;
  function jsonEntry(entry: JournalEntry): Record<string, unknown>;
  function textEntry(entry: JournalEntry): string;

  // -----------------------------------------------------------------------
  // Netdata function surface
  // -----------------------------------------------------------------------

  const NETDATA_SOURCE_TYPE_ALL: number;
  const NETDATA_SOURCE_TYPE_LOCAL_ALL: number;
  const NETDATA_SOURCE_TYPE_REMOTE_ALL: number;
  const NETDATA_SOURCE_TYPE_LOCAL_SYSTEM: number;
  const NETDATA_SOURCE_TYPE_LOCAL_USER: number;
  const NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE: number;
  const NETDATA_SOURCE_TYPE_LOCAL_OTHER: number;

  const DEFAULT_FUNCTION_NAME: string;
  const DEFAULT_SOURCE_SELECTOR_NAME: string;
  const DEFAULT_SOURCE_SELECTOR_HELP: string;
  const DEFAULT_ITEMS_TO_RETURN: number;
  const DEFAULT_TIME_WINDOW_SECONDS: number;
  const DEFAULT_ITEMS_SAMPLING: number;
  const DEFAULT_HISTOGRAM_BUCKETS: number;

  interface NetdataFunctionConfigOptions {
    functionName?: string;
    sourceSelectorName?: string;
    sourceSelectorHelp?: string;
    defaultFacets?: string[];
    defaultViewKeys?: string[];
    defaultHistogram?: string;
    readerOptions?: ReaderOptions | null;
    explorerStrategy?: ExplorerStrategy | null;
  }

  class NetdataFunctionConfig {
    constructor(opts?: NetdataFunctionConfigOptions);
    functionName: string;
    sourceSelectorName: string;
    sourceSelectorHelp: string;
    defaultFacets: string[];
    defaultViewKeys: string[];
    defaultHistogram: string;
    readerOptions: ReaderOptions | null;
    explorerStrategy: ExplorerStrategy | null;
    static systemdJournal(): NetdataFunctionConfig;
    backfillDefaults(): this;
  }

  class DisplayContext {
    _bootFirstRealtime: Map<string, bigint>;
    _uidCache: Map<string, string>;
    _gidCache: Map<string, string>;
    registerBootFirstRealtime(bootIdBytes: string | Bytes, realtimeUsec: bigint | number): void;
  }

  const DisplayScope: { readonly Data: "data"; readonly Facet: "facet"; readonly Histogram: "histogram" };

  class NetdataFunctionProfile {
    fieldDisplayValue(context: DisplayContext, scope: string, field: string, value: string | Bytes): string;
    facetOptionName(context: DisplayContext, field: string, rawValue: string | Bytes): string;
    rowOptions(fields: Record<string, string[]>): { severity: string };
  }

  class SystemdJournalProfile extends NetdataFunctionProfile {}
  class SystemdJournalPluginProfile extends NetdataFunctionProfile {}

  function priorityToRowSeverity(raw: string | Bytes): string;

  class NetdataRequest {
    constructor();
    info: boolean;
    echo: Record<string, unknown>;
    afterRealtimeUsec: number | null;
    beforeRealtimeUsec: number | null;
    ifModifiedSinceUsec: number;
    anchor: ExplorerAnchor;
    direction: Direction;
    limit: number;
    dataOnly: boolean;
    delta: boolean;
    tail: boolean;
    sampling: number;
    sourceType: number;
    exactSources: string[];
    filters: ExplorerFilter[];
    facets: Bytes[];
    histogram: string | null;
    query: string | null;

    static parse(
      value: Record<string, unknown>,
      config: NetdataFunctionConfig,
      injectableNow?: number,
    ): NetdataRequest;
  }

  class CombinedResult {
    constructor(options?: { readerOptions?: ReaderOptions | null });

    rows: any[];
    facets: Map<string, Map<string, bigint>>;
    histogram: { field: Bytes; buckets: any[] } | null;
    columnFields: Set<string>;
    stats: ExplorerStats;
    matchedFiles: number;
    matchedPaths: string[];
    skippedFiles: number;
    fileErrors: string[];
    partial: boolean;
    timedOut: boolean;
    cancelled: boolean;
    samplingEnabled: boolean;
    readerOptions: ReaderOptions | null;

    merge(path: string, result: ExplorerResult, direction: Direction, limit: number): void;
  }

  class JournalFileCollection {
    files: string[];
    skipped: number;
    errors: string[];
  }

  function normalizeTimeWindow(
    nowSeconds: number | null,
    after: number | null,
    before: number | null,
    injectableNow?: number,
  ): [number, number];

  function journalFileSourceType(pathStr: string): number;
  function collectJournalFiles(directory: string): JournalFileCollection;

  class NetdataJournalFileMetadata {
    sourceType: number | null;
    sourceName: string | null;
    fileLastModifiedUsec: bigint | null;
    msgFirstRealtimeUsec: bigint | null;
    msgLastRealtimeUsec: bigint | null;
    journalVsRealtimeDeltaUsec: bigint | null;
  }

  class NetdataFunctionState {
    fileMetadata(path: string): NetdataJournalFileMetadata | null;
    updateFileJournalVsRealtimeDeltaUsec(path: string, deltaUsec: bigint | number): void;
  }

  class NetdataFunctionProgress {
    currentFile: string | null;
    totalFiles: number;
    matchedFiles: number;
    skippedFiles: number;
    stats: ExplorerStats;
    elapsed: number;
  }

  class NetdataFunctionRunOptions {
    timeout: number | null;
    progressCallback: ((p: NetdataFunctionProgress) => void) | null;
    cancellationCallback: (() => boolean) | null;
    state: NetdataFunctionState | null;
    progressInterval: number;

    static fromTimeoutSeconds(seconds: number): NetdataFunctionRunOptions;
  }

  class NetdataJournalFunction {
    constructor(config: NetdataFunctionConfig, profile: NetdataFunctionProfile);

    static systemdJournal(): NetdataJournalFunction;
    static systemdJournalPluginCompatible(): NetdataJournalFunction;
    static new(config: NetdataFunctionConfig, profile: NetdataFunctionProfile): NetdataJournalFunction;

    runDirectoryRequestJson(directory: string, request: Record<string, unknown>): Record<string, unknown>;
    runDirectoryRequestJsonWithOptions(directory: string, request: Record<string, unknown>, options?: NetdataFunctionRunOptions | null): Record<string, unknown>;
    runDirectoryRequestBytes(directory: string, request: string | Bytes): Record<string, unknown>;
    runDirectoryRequestBytesWithOptions(directory: string, request: string | Bytes, options?: NetdataFunctionRunOptions | null): Record<string, unknown>;
  }

  // -----------------------------------------------------------------------
  // Verification
  // -----------------------------------------------------------------------

  function verifyFile(path: string, options?: ReaderOptions): void;
  function verifyFileWithKey(path: string, verificationKey: Bytes, options?: ReaderOptions): void;

  // -----------------------------------------------------------------------
  // Convenience factories
  // -----------------------------------------------------------------------

  function openJournal(path: string, options?: ReaderOptions): SdJournal;
  function createJournal(path: string, options?: WriterOptions): Writer;
  function stringField(name: string, value: string): Field;
  function binaryField(name: string | Bytes, value: Bytes): Field;

  // -----------------------------------------------------------------------
  // Verification errors
  // -----------------------------------------------------------------------

  class VerificationError extends Error {}

  // -----------------------------------------------------------------------
  // Forward Secure Sealing
  // -----------------------------------------------------------------------

  function fsprgGenMK(seed: Bytes, secpar?: number): Bytes;
  function fsprgGenState0(mpk: Bytes, seed: Bytes): Bytes;
  function fsprgEvolve(state: Bytes): Bytes;
  function fsprgSeek(state: Bytes, epoch: bigint, msk: Bytes, seed: Bytes): Bytes;
  function fsprgGetKey(state: Bytes, keylen: number, idx: bigint): Bytes;
  function fsprgGetEpoch(state: Bytes): bigint;

  // -----------------------------------------------------------------------
  // Writer lock
  // -----------------------------------------------------------------------

  class WriterLock {
    static acquire(path: string): WriterLock;
    release(): void;
  }
}
