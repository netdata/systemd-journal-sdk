#!/usr/bin/env node
// Node.js reader-core benchmark command.

import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import {
  DirectoryReader,
  FileReader,
  SdJournalEnumerateAvailableData,
  SdJournalNext,
  SdJournalOpenDirectory,
  SdJournalOpenFiles,
  SdJournalPrevious,
  SdJournalRestartData,
} from '../src/index.js';

const MASK64 = (1n << 64n) - 1n;

class Counts {
  constructor() {
    this.records = 0;
    this.fields = 0;
    this.bytes = 0;
    this.checksum = 0n;
  }

  addPayload(payload) {
    this.fields += 1;
    this.bytes += payload.length;
    this.checksum = checksumPayload(this.checksum, payload);
  }

  addRecordMarker(value) {
    this.records += 1;
    this.checksum = rotateLeft(this.checksum, 7n) ^ BigInt(value);
    this.checksum &= MASK64;
  }
}

function rotateLeft(value, shift) {
  value &= MASK64;
  return ((value << shift) | (value >> (64n - shift))) & MASK64;
}

function checksumPayload(checksum, payload) {
  checksum = rotateLeft(checksum, 5n) ^ BigInt(payload.length);
  if (payload.length > 0) {
    checksum ^= BigInt(payload[0]) << 8n;
    checksum ^= BigInt(payload[payload.length - 1]);
  }
  return checksum & MASK64;
}

function processStatusKb() {
  let status;
  try {
    status = readFileSync('/proc/self/status', 'utf8');
  } catch {
    return {};
  }
  const wanted = new Set([
    'VmSize', 'VmPeak', 'VmRSS', 'VmHWM', 'RssAnon', 'RssFile',
    'RssShmem', 'VmData', 'VmStk', 'VmExe', 'VmLib', 'VmPTE',
  ]);
  const out = {};
  for (const line of status.split('\n')) {
    const idx = line.indexOf(':');
    if (idx < 0) continue;
    const key = line.slice(0, idx);
    if (!wanted.has(key)) continue;
    const parts = line.slice(idx + 1).trim().split(/\s+/);
    if (parts.length > 0) {
      const value = Number(parts[0]);
      if (Number.isFinite(value)) out[`${key}_kb`] = value;
    }
  }
  return out;
}

function advance(reader, direction) {
  return direction === 'backward' ? reader.previous() : reader.next();
}

function openSdkReader(inputs, surface) {
  if (surface === 'file') {
    if (inputs.length !== 1) throw new Error('file surface requires exactly one --input');
    return FileReader.open(inputs[0]);
  }
  if (surface === 'directory') {
    if (inputs.length !== 1) throw new Error('directory surface requires exactly one --input');
    return DirectoryReader.open(inputs[0]);
  }
  if (surface === 'open-files') {
    return DirectoryReader.openFiles(inputs);
  }
  throw new Error(`invalid surface: ${surface}`);
}

function seekReader(reader, direction) {
  if (direction === 'backward') reader.seekTail();
  else reader.seekHead();
}

function readSdk(inputs, surface, mode, direction) {
  const reader = openSdkReader(inputs, surface);
  try {
    seekReader(reader, direction);
    const counts = new Counts();
    while (advance(reader, direction)) {
      if (mode === 'sdk-entry') {
        const entry = reader.getEntry();
        counts.addRecordMarker(entry.realtime);
        for (const payload of entry.payloads) counts.addPayload(payload);
      } else if (mode === 'sdk-payloads') {
        counts.addRecordMarker(reader.getRealtimeUsec());
        reader.visitEntryPayloads((payload) => counts.addPayload(payload));
      } else {
        throw new Error(`invalid SDK mode: ${mode}`);
      }
    }
    return counts;
  } finally {
    reader.close();
  }
}

function openFacade(inputs, surface) {
  if (surface === 'file' || surface === 'open-files') return SdJournalOpenFiles(inputs, 0);
  if (surface === 'directory') {
    if (inputs.length !== 1) throw new Error('directory surface requires exactly one --input');
    return SdJournalOpenDirectory(inputs[0], 0);
  }
  throw new Error(`invalid facade surface: ${surface}`);
}

function readFacade(inputs, surface, mode, direction) {
  const journal = openFacade(inputs, surface);
  try {
    seekReader(journal, direction);
    const counts = new Counts();
    for (;;) {
      const advanced = direction === 'backward' ? SdJournalPrevious(journal) : SdJournalNext(journal);
      if (advanced === 0) break;
      if (mode === 'facade-next') {
        counts.addRecordMarker(journal.getRealtimeUsec());
      } else if (mode === 'facade-data') {
        counts.addRecordMarker(journal.getRealtimeUsec());
        SdJournalRestartData(journal);
        for (;;) {
          const payload = SdJournalEnumerateAvailableData(journal);
          if (payload === null) break;
          counts.addPayload(payload);
        }
      } else {
        throw new Error(`invalid facade mode: ${mode}`);
      }
    }
    return counts;
  } finally {
    journal.close();
  }
}

function parseArgs(argv) {
  const args = Object.assign(Object.create(null), {
    inputs: [],
    mode: 'sdk-payloads',
    surface: 'file',
    direction: 'forward',
    windowSize: '0',
    bounds: 'live',
    mmapStrategy: 'buffer',
  });
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) throw new Error(`missing value for ${arg}`);
      return argv[++i];
    };
    if (arg === '--input') args.inputs.push(next());
    else if (arg === '--mode') args.mode = next();
    else if (arg === '--surface') args.surface = next();
    else if (arg === '--direction') args.direction = next();
    else if (arg === '--window-size') args.windowSize = next();
    else if (arg === '--bounds') args.bounds = next();
    else if (arg === '--mmap-strategy') args.mmapStrategy = next();
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (args.inputs.length === 0) throw new Error('missing --input');
  return args;
}

function stringifyResult(result) {
  const checksum = result.checksum;
  const json = JSON.stringify({ ...result, checksum: '__CHECKSUM__' });
  return json.replace('"__CHECKSUM__"', checksum.toString());
}

function run(args) {
  const statusBefore = processStatusKb();
  const started = performance.now();
  let counts;
  if (args.mode === 'sdk-entry' || args.mode === 'sdk-payloads') {
    counts = readSdk(args.inputs, args.surface, args.mode, args.direction);
  } else if (args.mode === 'facade-next' || args.mode === 'facade-data') {
    counts = readFacade(args.inputs, args.surface, args.mode, args.direction);
  } else {
    throw new Error(`invalid --mode for Node.js reader benchmark: ${args.mode}`);
  }
  const readSeconds = (performance.now() - started) / 1000;
  const statusAfter = processStatusKb();
  return { counts, readSeconds, statusBefore, statusAfter };
}

try {
  const args = parseArgs(process.argv);
  const { counts, readSeconds, statusBefore, statusAfter } = run(args);
  const result = {
    language: 'node',
    surface: args.surface,
    mode: args.mode,
    direction: args.direction,
    records: counts.records,
    fields: counts.fields,
    bytes: counts.bytes,
    checksum: counts.checksum,
    read_seconds: readSeconds,
    read_rows_per_second: readSeconds > 0 ? counts.records / readSeconds : 0.0,
    read_fields_per_second: readSeconds > 0 ? counts.fields / readSeconds : 0.0,
    read_bytes_per_second: readSeconds > 0 ? counts.bytes / readSeconds : 0.0,
    inputs: args.inputs,
    window_size: args.windowSize,
    bounds: args.bounds,
    mmap_strategy: args.mmapStrategy,
    timer_excludes: ['fixture generation', 'process startup', 'external verification'],
    process_status_before: statusBefore,
    process_status_after: statusAfter,
    errors: [],
  };
  process.stdout.write(stringifyResult(result) + '\n');
} catch (error) {
  process.stderr.write(`${error.stack || error.message}\n`);
  process.exitCode = 1;
}
