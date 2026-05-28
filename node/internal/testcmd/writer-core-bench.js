#!/usr/bin/env node

import { mkdirSync, rmSync, statSync } from 'node:fs';
import { dirname } from 'node:path';
import { performance } from 'node:perf_hooks';
import { Writer } from '../../src/index.js';

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
    output: '',
    maxSizeBytes: DEFAULT_MAX_SIZE_BYTES,
    livePublishEveryEntries: 1,
  };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === '--rows' && next !== undefined) {
      args.rows = Number(next);
      i++;
    } else if (arg === '--output' && next !== undefined) {
      args.output = next;
      i++;
    } else if (arg === '--format' && next !== undefined) {
      args.format = next;
      i++;
    } else if (arg === '--final-state' && next !== undefined) {
      args.finalState = next;
      i++;
    } else if (arg === '--max-size-bytes' && next !== undefined) {
      args.maxSizeBytes = Number(next);
      i++;
    } else if (arg === '--live-publish-every-entries' && next !== undefined) {
      args.livePublishEveryEntries = Number(next);
      i++;
    } else {
      throw new Error(`unknown or incomplete argument: ${arg}`);
    }
  }
  return args;
}

function dataHashBucketsForMaxSize(maxSizeBytes) {
  // Keep this driver aligned with header.js and systemd's max_size * 4 / 768 / 3 formula.
  const buckets = Math.floor(maxSizeBytes / 576);
  return Math.max(buckets, 2047);
}

function makeRows(rows) {
  const fixed = [
    { name: 'TEST_ID', value: Buffer.from('deterministic-ingestion-performance') },
    { name: 'PERF_PROFILE', value: Buffer.from('mixed-cardinality-32-fields') },
    { name: 'HOST_CLASS', value: Buffer.from('synthetic-edge') },
    { name: 'SOURCE_KIND', value: Buffer.from('journal-sdk-benchmark') },
  ];
  const lowValues = Array.from({ length: 12 }, (_, offset) =>
    Array.from({ length: 16 }, (_, value) => Buffer.from(`low-${offset.toString().padStart(2, '0')}-${value.toString().padStart(2, '0')}`))
  );
  const mediumValues = Array.from({ length: 8 }, (_, offset) =>
    Array.from({ length: 2048 }, (_, value) => Buffer.from(`medium-${offset.toString().padStart(2, '0')}-${value.toString().padStart(4, '0')}`))
  );

  const all = new Array(rows);
  for (let row = 0; row < rows; row++) {
    const fields = fixed.slice();
    for (let offset = 0; offset < 12; offset++) {
      fields.push({
        name: `LOW_CARD_${offset.toString().padStart(2, '0')}`,
        value: lowValues[offset][row % 16],
      });
    }
    for (let offset = 0; offset < 8; offset++) {
      fields.push({
        name: `MED_CARD_${offset.toString().padStart(2, '0')}`,
        value: mediumValues[offset][row % 2048],
      });
    }
    for (let offset = 0; offset < 8; offset++) {
      fields.push({
        name: `HIGH_CARD_${offset.toString().padStart(2, '0')}`,
        value: Buffer.from(`high-${offset.toString().padStart(2, '0')}-${row.toString().padStart(6, '0')}`),
      });
    }
    all[row] = fields;
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
    append_seconds: 0,
    append_rows_per_second: 0,
    close_seconds: 0,
    total_writer_seconds: 0,
    precompute_seconds: 0,
    journal_size_bytes: 0,
    journal_path: '',
    format: args.format,
    compression: 'none',
    fss: false,
    api_mode: 'field-api',
    data_hash_table_buckets: dataHashBuckets,
    field_hash_table_buckets: FIELD_HASH_BUCKETS,
    max_size_bytes: args.maxSizeBytes,
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

  const precomputeStart = performance.now();
  const rows = makeRows(args.rows);
  result.precompute_seconds = (performance.now() - precomputeStart) / 1000;

  mkdirSync(dirname(args.output), { recursive: true });
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
    writer.append(rows[index], {
      realtimeUsec: BASE_REALTIME_USEC + BigInt(index) * 500n,
      monotonicUsec: BASE_MONOTONIC_USEC + BigInt(index) * 50n,
      bootId: BOOT_ID,
    });
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
  result.journal_size_bytes = statSync(journalPath).size;
  emit(result, result.records === args.rows ? 0 : 1);
} catch (error) {
  emit({
    records: 0,
    fields_per_row: FIELDS_PER_ROW,
    append_seconds: 0,
    append_rows_per_second: 0,
    close_seconds: 0,
    total_writer_seconds: 0,
    precompute_seconds: 0,
    journal_size_bytes: 0,
    journal_path: '',
    format: 'unknown',
    compression: 'none',
    fss: false,
    api_mode: 'field-api',
    data_hash_table_buckets: 0,
    field_hash_table_buckets: FIELD_HASH_BUCKETS,
    max_size_bytes: DEFAULT_MAX_SIZE_BYTES,
    live_publish_every_entries: 1,
    append_timer_excludes: ['row generation', 'writer creation', 'final close/sync', 'journal verification'],
    final_state: 'unknown',
    errors: [error && error.stack ? error.stack : String(error)],
  }, 1);
}
