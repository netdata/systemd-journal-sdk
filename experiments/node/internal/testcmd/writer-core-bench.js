#!/usr/bin/env node

import { rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { Log, LOG_IDENTITY_STRICT, Writer } from '../../src/index.js';
import { safeMkdirSync, safeReaddirSync, safeStatSync } from '../../src/lib/fs-safe.js';

const BASE_REALTIME_USEC = 1700000000000000n;
const BASE_MONOTONIC_USEC = 50000000n;
const SEQNUM_ID_HEX = '22222222222222222222222222222222';
const FIELDS_PER_ROW = 32;
const DEFAULT_MAX_SIZE_BYTES = 128 * 1024 * 1024;
const FIELD_HASH_BUCKETS = 1023;

const BOOT_ID = Buffer.from('0123456789abcdef0123456789abcdef', 'hex');
const MACHINE_ID = Buffer.from('fedcba9876543210fedcba9876543210', 'hex');
const SEQNUM_ID = Buffer.from(SEQNUM_ID_HEX, 'hex');
const FILE_ID = Buffer.from('33333333333333333333333333333333', 'hex');

function parseArgs(argv) {
  const args = {
    rows: 100000,
    format: 'compact',
    finalState: 'online',
    surface: 'direct',
    output: '',
    maxSizeBytes: DEFAULT_MAX_SIZE_BYTES,
    rotationMaxSizeBytes: DEFAULT_MAX_SIZE_BYTES,
    livePublishEveryEntries: 1,
    apiMode: 'raw-payload',
  };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv.at(i);
    i = parseWriterBenchArg(args, argv, i, arg);
  }
  return args;
}

const WRITER_BENCH_STRING_OPTIONS = new Map([
  ['--output', 'output'],
  ['--format', 'format'],
  ['--final-state', 'finalState'],
  ['--surface', 'surface'],
  ['--api-mode', 'apiMode'],
]);

const WRITER_BENCH_NUMBER_OPTIONS = new Map([
  ['--rows', 'rows'],
  ['--max-size-bytes', 'maxSizeBytes'],
  ['--rotation-max-size-bytes', 'rotationMaxSizeBytes'],
  ['--live-publish-every-entries', 'livePublishEveryEntries'],
]);

function parseWriterBenchArg(args, argv, index, arg) {
  const next = argv[index + 1];
  if (next === undefined) throw new Error(`unknown or incomplete argument: ${arg}`);
  const stringField = WRITER_BENCH_STRING_OPTIONS.get(arg);
  if (stringField) {
    Reflect.set(args, stringField, next);
    return index + 1;
  }
  const numberField = WRITER_BENCH_NUMBER_OPTIONS.get(arg);
  if (numberField) {
    Reflect.set(args, numberField, Number(next));
    return index + 1;
  }
  throw new Error(`unknown or incomplete argument: ${arg}`);
}

function dataHashBucketsForMaxSize(maxSizeBytes) {
  // Keep this driver aligned with header.js and systemd's max_size * 4 / 768 / 3 formula.
  const buckets = Math.floor(maxSizeBytes / 576);
  return Math.max(buckets, 2047);
}

function livePublicationName(everyEntries) {
  if (everyEntries === 0) return 'disabled';
  if (everyEntries === 1) return 'immediate';
  return `every-n:${everyEntries}`;
}

function fieldWithPayload(name, value) {
  const normalized = Buffer.isBuffer(value) ? value : Buffer.from(String(value));
  return {
    field: { name, value: normalized },
    payload: Buffer.concat([Buffer.from(`${name}=`, 'utf8'), normalized]),
  };
}

function makeRows(rows) {
  const fixed = [
    fieldWithPayload('TEST_ID', Buffer.from('deterministic-ingestion-performance')),
    fieldWithPayload('PERF_PROFILE', Buffer.from('mixed-cardinality-32-fields')),
    fieldWithPayload('HOST_CLASS', Buffer.from('synthetic-edge')),
    fieldWithPayload('SOURCE_KIND', Buffer.from('journal-sdk-benchmark')),
  ];
  const lowValues = Array.from({ length: 12 }, (_, offset) =>
    Array.from({ length: 16 }, (_, value) => Buffer.from(`low-${offset.toString().padStart(2, '0')}-${value.toString().padStart(2, '0')}`))
  );
  const mediumValues = Array.from({ length: 8 }, (_, offset) =>
    Array.from({ length: 2048 }, (_, value) => Buffer.from(`medium-${offset.toString().padStart(2, '0')}-${value.toString().padStart(4, '0')}`))
  );

  const all = [];
  for (let row = 0; row < rows; row++) {
    const fields = fixed.slice();
    for (let offset = 0; offset < 12; offset++) {
      fields.push(fieldWithPayload(
        `LOW_CARD_${offset.toString().padStart(2, '0')}`,
        lowValues.at(offset).at(row % 16),
      ));
    }
    for (let offset = 0; offset < 8; offset++) {
      fields.push(fieldWithPayload(
        `MED_CARD_${offset.toString().padStart(2, '0')}`,
        mediumValues.at(offset).at(row % 2048),
      ));
    }
    for (let offset = 0; offset < 8; offset++) {
      fields.push(fieldWithPayload(
        `HIGH_CARD_${offset.toString().padStart(2, '0')}`,
        Buffer.from(`high-${offset.toString().padStart(2, '0')}-${row.toString().padStart(6, '0')}`),
      ));
    }
    all.push({
      fields: fields.map((item) => item.field),
      payloads: fields.map((item) => item.payload),
    });
  }
  return all;
}

function archivePathFor(output) {
  const prefix = output.endsWith('.journal') ? output.slice(0, -'.journal'.length) : output;
  return `${prefix}@${SEQNUM_ID_HEX}-0000000000000001-${BASE_REALTIME_USEC.toString(16).padStart(16, '0')}.journal`;
}

function closeWriter(writer, output, finalState) {
  if (finalState === 'online') {
    writer.close();
    return output;
  }
  if (finalState === 'offline') {
    writer.closeOffline();
    return output;
  }
  if (finalState === 'archived') {
    const archivePath = archivePathFor(output);
    rmSync(archivePath, { force: true });
    writer.archiveTo(archivePath);
    return archivePath;
  }
  throw new Error(`invalid final state: ${finalState}`);
}

function collectJournalFiles(root) {
  const files = [];
  let total = 0;
  const walk = (dir) => {
    for (const name of safeReaddirSync(dir)) {
      const path = join(dir, name);
      const st = safeStatSync(path);
      if (st.isDirectory()) {
        walk(path);
      } else if (name.endsWith('.journal')) {
        files.push(path);
        total += st.size;
      }
    }
  };
  walk(root);
  return { files, total };
}

function runDirectory(result, args, rows) {
  rmSync(args.output, { force: true, recursive: true });
  const log = new Log(args.output, {
    source: 'system',
    machineId: MACHINE_ID,
    bootId: BOOT_ID,
    seqnumId: SEQNUM_ID,
    headSeqnum: 1,
    identityMode: LOG_IDENTITY_STRICT,
    compression: 0,
    compressionThresholdBytes: 512,
    compact: args.format === 'compact',
    livePublishEveryEntries: args.livePublishEveryEntries,
    rotationPolicy: { maxFileSize: args.rotationMaxSizeBytes },
  });

  const appendStart = performance.now();
  for (let index = 0; index < rows.length; index++) {
    const appendOptions = {
      realtimeUsec: BASE_REALTIME_USEC + BigInt(index) * 500n,
      monotonicUsec: BASE_MONOTONIC_USEC + BigInt(index) * 50n,
      bootId: BOOT_ID,
    };
    if (args.apiMode === 'raw-payload') {
      log.appendRaw(rows.at(index).payloads, appendOptions);
    } else {
      log.append(rows.at(index).fields, appendOptions);
    }
    result.records++;
  }
  result.append_seconds = (performance.now() - appendStart) / 1000;
  if (result.append_seconds > 0) {
    result.append_rows_per_second = result.records / result.append_seconds;
  }

  const closeStart = performance.now();
  log.close();
  result.close_seconds = (performance.now() - closeStart) / 1000;
  result.total_writer_seconds = result.append_seconds + result.close_seconds;
  result.journal_directory = log.journalDirectory();
  result.journal_path = result.journal_directory;
  const collected = collectJournalFiles(args.output);
  result.journal_files = collected.files;
  result.journal_size_bytes = collected.total;
}

function emit(result, exitCode) {
  console.log(JSON.stringify(result));
  process.exit(exitCode);
}

try {
  const args = parseArgs(process.argv);
  const dataHashBuckets = dataHashBucketsForMaxSize(args.maxSizeBytes);
  const result = {
    records: 0,
    fields_per_row: FIELDS_PER_ROW,
    surface: args.surface,
    append_seconds: 0,
    append_rows_per_second: 0,
    close_seconds: 0,
    total_writer_seconds: 0,
    precompute_seconds: 0,
    journal_size_bytes: 0,
    journal_path: '',
    journal_directory: '',
    journal_files: [],
    format: args.format,
    compression: 'none',
    fss: false,
    api_mode: args.apiMode,
    data_hash_table_buckets: dataHashBuckets,
    field_hash_table_buckets: FIELD_HASH_BUCKETS,
    max_size_bytes: args.maxSizeBytes,
    rotation_max_size_bytes: args.rotationMaxSizeBytes,
    live_publication: livePublicationName(args.livePublishEveryEntries),
    live_publish_every_entries: args.livePublishEveryEntries,
    append_timer_excludes: ['row generation', 'writer creation', 'final close/sync', 'journal verification'],
    final_state: args.finalState,
    errors: [],
  };
  if (!args.output) {
    result.errors.push('--output is required');
    emit(result, 2);
  }
  const compact = args.format === 'compact';
  if (!compact && args.format !== 'regular') {
    result.errors.push('invalid --format');
    emit(result, 2);
  }
  if (args.apiMode !== 'raw-payload' && args.apiMode !== 'structured-field') {
    result.errors.push('invalid --api-mode');
    emit(result, 2);
  }
  if (args.surface !== 'direct' && args.surface !== 'directory') {
    result.errors.push('invalid --surface');
    emit(result, 2);
  }

  const precomputeStart = performance.now();
  const rows = makeRows(args.rows);
  result.precompute_seconds = (performance.now() - precomputeStart) / 1000;

  if (args.surface === 'directory') {
    runDirectory(result, args, rows);
    emit(result, result.records === args.rows && result.errors.length === 0 ? 0 : 1);
  }

  safeMkdirSync(dirname(args.output), { recursive: true });
  rmSync(args.output, { force: true });
  const writer = Writer.create(args.output, {
    machineId: MACHINE_ID,
    bootId: BOOT_ID,
    seqnumId: SEQNUM_ID,
    fileId: FILE_ID,
    headSeqnum: 1,
    compression: 0,
    compressionThresholdBytes: 512,
    dataHashTableBuckets: dataHashBuckets,
    fieldHashTableBuckets: FIELD_HASH_BUCKETS,
    livePublishEveryEntries: args.livePublishEveryEntries,
    compact,
  });

  const appendStart = performance.now();
  for (let index = 0; index < rows.length; index++) {
    const appendOptions = {
      realtimeUsec: BASE_REALTIME_USEC + BigInt(index) * 500n,
      monotonicUsec: BASE_MONOTONIC_USEC + BigInt(index) * 50n,
      bootId: BOOT_ID,
    };
    if (args.apiMode === 'raw-payload') {
      writer.appendRaw(rows.at(index).payloads, appendOptions);
    } else {
      writer.append(rows.at(index).fields, appendOptions);
    }
    result.records++;
  }
  result.append_seconds = (performance.now() - appendStart) / 1000;
  if (result.append_seconds > 0) {
    result.append_rows_per_second = result.records / result.append_seconds;
  }

  const closeStart = performance.now();
  const journalPath = closeWriter(writer, args.output, args.finalState);
  result.close_seconds = (performance.now() - closeStart) / 1000;
  result.total_writer_seconds = result.append_seconds + result.close_seconds;
  result.journal_path = journalPath;
  result.journal_size_bytes = safeStatSync(journalPath).size;
  emit(result, result.records === args.rows ? 0 : 1);
} catch (error) {
  emit({
    records: 0,
    fields_per_row: FIELDS_PER_ROW,
    surface: 'unknown',
    append_seconds: 0,
    append_rows_per_second: 0,
    close_seconds: 0,
    total_writer_seconds: 0,
    precompute_seconds: 0,
    journal_size_bytes: 0,
    journal_path: '',
    journal_directory: '',
    journal_files: [],
    format: 'unknown',
    compression: 'none',
    fss: false,
    api_mode: 'unknown',
    data_hash_table_buckets: 0,
    field_hash_table_buckets: FIELD_HASH_BUCKETS,
    max_size_bytes: DEFAULT_MAX_SIZE_BYTES,
    rotation_max_size_bytes: DEFAULT_MAX_SIZE_BYTES,
    live_publication: 'immediate',
    live_publish_every_entries: 1,
    append_timer_excludes: ['row generation', 'writer creation', 'final close/sync', 'journal verification'],
    final_state: 'unknown',
    errors: [error && error.stack ? error.stack : String(error)],
  }, 1);
}
