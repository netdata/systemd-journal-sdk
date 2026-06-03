#!/usr/bin/env node
// Deterministic dataset ingester for the JavaScript journal writer.

import { readFileSync, rmSync } from 'node:fs';
import { mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Writer } from '../src/lib/writer.js';

const BOOT_ID = Buffer.from('0123456789abcdef0123456789abcdef', 'hex');
const MACHINE_ID = Buffer.from('fedcba9876543210fedcba9876543210', 'hex');
const SEQNUM_ID = Buffer.from('22222222222222222222222222222222', 'hex');
const FILE_ID = Buffer.from('33333333333333333333333333333333', 'hex');
const OVERSIZED_LIMIT = 4 * 1024 * 1024;
const DEFAULT_ARCHIVE_REALTIME = 1_700_000_000_000_000n;

function materializeValue(value) {
  if (value.kind === 'utf8') return Buffer.from(value.text, 'utf8');
  if (value.kind === 'bytes') {
    const data = Buffer.from(value.base64, 'base64');
    if (value.size !== undefined && data.length !== value.size) {
      throw new Error(`bytes size mismatch: expected ${value.size}, got ${data.length}`);
    }
    return data;
  }
  if (value.kind === 'repeat') return Buffer.alloc(value.size, value.byte);
  throw new Error(`unknown value kind: ${value.kind}`);
}

function validFieldName(name) {
  if (!name || Buffer.byteLength(name, 'utf8') > 64) return false;
  const first = name.charCodeAt(0);
  if (first >= 0x30 && first <= 0x39) return false;
  for (let i = 0; i < name.length; i++) {
    const ch = name.charCodeAt(i);
    if (ch !== 0x5f && !(ch >= 0x41 && ch <= 0x5a) && !(ch >= 0x30 && ch <= 0x39)) {
      return false;
    }
  }
  return true;
}

function expectedRejection(input) {
  if (input.raw_payload !== undefined) {
    const eq = input.raw_payload.indexOf('=');
    if (eq < 0) return 'EINVAL';
    const name = input.raw_payload.slice(0, eq);
    return validFieldName(name) ? null : 'EINVAL';
  }
  const name = input.field_name;
  if (name === undefined || !validFieldName(name)) return 'EINVAL';
  const value = input.value;
  if (value === null || value === undefined) return 'EINVAL';
  if (value.kind === 'repeat' && value.size > OVERSIZED_LIMIT) return 'E2BIG';
  return null;
}

function makeWriter(path, compact, maxSizeBytes) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- output path is the explicit CLI target.
  mkdirSync(dirname(path), { recursive: true });
  const options = {
    bootId: BOOT_ID,
    machineId: MACHINE_ID,
    seqnumId: SEQNUM_ID,
    fileId: FILE_ID,
    headSeqnum: 1,
    compression: 'none',
    compressionThresholdBytes: 512,
    compact,
  };
  if (maxSizeBytes !== undefined) options.maxFileSize = maxSizeBytes;
  return Writer.create(path, options);
}

function archivePathFor(output, headRealtime) {
  const prefix = output.endsWith('.journal') ? output.slice(0, -'.journal'.length) : output;
  return `${prefix}@${SEQNUM_ID.toString('hex')}-0000000000000001-${headRealtime.toString(16).padStart(16, '0')}.journal`;
}

function finalizeWriter(writer, output, finalState, headRealtime) {
  if (finalState === 'online') writer.close();
  else if (finalState === 'offline') writer.closeOffline();
  else if (finalState === 'archived') {
    const archivePath = archivePathFor(output, headRealtime);
    rmSync(archivePath, { force: true });
    writer.archiveTo(archivePath);
  }
  else throw new Error(`invalid final state: ${finalState}`);
}

function records(path) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- dataset path is the explicit CLI input.
  return readFileSync(path, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line));
}

function ingestAccepted(dataset, output, finalState, compact, maxSizeBytes) {
  const writer = makeWriter(output, compact, maxSizeBytes);
  let written = 0;
  let headRealtime = 0n;
  const errors = [];
  try {
    for (const [index, record] of records(dataset).entries()) {
      if (record.record_type !== 'accepted') continue;
      const fields = record.fields.map(item => ({
        name: item.name,
        value: materializeValue(item.value),
      }));
      try {
        writer.append(fields, {
          realtimeUsec: BigInt(record.realtime_usec),
          monotonicUsec: BigInt(record.monotonic_usec),
          bootId: Buffer.from(record.boot_id || BOOT_ID.toString('hex'), 'hex'),
        });
        if (headRealtime === 0n) headRealtime = BigInt(record.realtime_usec);
        written++;
      } catch (error) {
        errors.push(`line ${index + 1}: append failed: ${error.message}`);
      }
    }
    writer.sync();
  } finally {
    finalizeWriter(writer, output, finalState, headRealtime || DEFAULT_ARCHIVE_REALTIME);
  }
  return { records: written, errors };
}

function ingestRejections(dataset, output, finalState, compact, maxSizeBytes) {
  let writer = null;
  let handled = 0;
  const errors = [];
  for (const [index, record] of records(dataset).entries()) {
    if (record.record_type !== 'rejected') continue;
    const expected = record.expected_error;
    const precheck = expectedRejection(record.input || {});
    if (precheck !== null) {
      if (precheck === expected) handled++;
      else errors.push(`line ${index + 1} ${record.case_id}: got ${precheck}, expected ${expected}`);
      continue;
    }

    writer ||= makeWriter(output, compact, maxSizeBytes);
    try {
      writer.append(
        [{ name: record.input.field_name, value: materializeValue(record.input.value) }],
        { bootId: BOOT_ID },
      );
      errors.push(`line ${index + 1} ${record.case_id}: unexpectedly accepted`);
    } catch {
      if (expected === 'EINVAL') handled++;
      else errors.push(`line ${index + 1} ${record.case_id}: rejected as EINVAL, expected ${expected}`);
    }
  }
  if (writer) finalizeWriter(writer, output, finalState, DEFAULT_ARCHIVE_REALTIME);
  return { records: handled, errors };
}

function parseArgs(argv) {
  const args = Object.assign(Object.create(null), {
    finalState: 'online',
    rejectionMode: false,
    compact: false,
  });
  for (let i = 2; i < argv.length; i++) {
    const arg = argv.at(i);
    i = parseIngesterArg(args, argv, i, arg);
  }
  validateIngesterArgs(args);
  return args;
}

function parseIngesterArg(args, argv, index, arg) {
  if (arg === '--rejection-mode') { args.rejectionMode = true; return index; }
  if (arg === '--compact') { args.compact = true; return index; }
  const value = argv.at(index + 1);
  if (value === undefined) throw new Error(`missing value for ${arg}`);
  if (arg === '--final-state') args.finalState = value;
  else if (arg === '--dataset') args.dataset = value;
  else if (arg === '--output') args.output = value;
  else if (arg === '--max-size-bytes') args.maxSizeBytes = Number(value);
  else throw new Error(`unknown argument: ${arg}`);
  return index + 1;
}

function validateIngesterArgs(args) {
  if (!['online', 'offline', 'archived'].includes(args.finalState)) throw new Error(`invalid final state: ${args.finalState}`);
  if (args.maxSizeBytes !== undefined && (!Number.isSafeInteger(args.maxSizeBytes) || args.maxSizeBytes <= 0)) throw new Error('invalid --max-size-bytes');
  if (!args.dataset || !args.output) throw new Error('usage: dataset_ingester --dataset PATH --output PATH [--rejection-mode] [--final-state online|offline|archived] [--compact] [--max-size-bytes BYTES]');
}

try {
  const args = parseArgs(process.argv);
  const result = args.rejectionMode
    ? ingestRejections(args.dataset, args.output, args.finalState, args.compact, args.maxSizeBytes)
    : ingestAccepted(args.dataset, args.output, args.finalState, args.compact, args.maxSizeBytes);
  console.log(JSON.stringify(result, Object.keys(result).sort()));
  process.exit(result.errors.length === 0 ? 0 : 1);
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
