#!/usr/bin/env node

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { SealOptions } from '../../src/lib/seal.js';
import { Writer } from '../../src/lib/writer.js';
import { WriterLock } from '../../src/lib/lock.js';

function parseArgs(argv) {
  const args = {
    path: '',
    readyFile: '',
    entries: 1000,
    delayMs: 1,
    syncEvery: 25,
    crashAfter: 0,
    binaryFixture: false,
    zstdFixture: false,
    lz4Fixture: false,
    xzFixture: false,
    compression: 'none',
    compressionThresholdBytes: 512,
    compact: false,
    seal: false,
    sealIntervalUsec: 1_000_000,
    sealStartUsec: 1_700_001_000_000_000,
  };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    i = parseLiveWriterArg(args, argv, i, arg);
  }

  if (!args.path || !args.readyFile || args.entries <= 0) {
    throw new Error('path, ready-file, and positive entries are required');
  }
  return args;
}

function parseLiveWriterArg(args, argv, index, arg) {
  if (parseLiveWriterFlag(args, arg)) return index;
  const value = argv[index + 1];
  if (value === undefined) throw new Error(`${arg} requires a value`);
  applyLiveWriterOption(args, arg, value);
  return index + 1;
}

function parseLiveWriterFlag(args, arg) {
  const flags = new Map([
    ['--binary-fixture', 'binaryFixture'],
    ['--zstd-fixture', 'zstdFixture'],
    ['--lz4-fixture', 'lz4Fixture'],
    ['--xz-fixture', 'xzFixture'],
    ['--compact', 'compact'],
    ['--seal', 'seal'],
  ]);
  const field = flags.get(arg);
  if (!field) return false;
  args[field] = true;
  return true;
}

function applyLiveWriterOption(args, arg, value) {
  switch (arg) {
    case '--path': args.path = value; return;
    case '--ready-file': args.readyFile = value; return;
    case '--entries': args.entries = parsePositiveInt(value, '--entries'); return;
    case '--delay': args.delayMs = parseDelayMs(value); return;
    case '--sync-every': args.syncEvery = parseNonNegativeInt(value, '--sync-every'); return;
    case '--crash-after': args.crashAfter = parseNonNegativeInt(value, '--crash-after'); return;
    case '--compression': args.compression = value; return;
    case '--compress-threshold':
    case '--compression-threshold-bytes':
      args.compressionThresholdBytes = parsePositiveInt(value, '--compression-threshold-bytes');
      return;
    case '--seal-interval-usec': args.sealIntervalUsec = parsePositiveInt(value, '--seal-interval-usec'); return;
    case '--seal-start-usec': args.sealStartUsec = parsePositiveInt(value, '--seal-start-usec'); return;
    default: throw new Error(`unknown argument: ${arg}`);
  }
}

function parsePositiveInt(value, name) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function parseNonNegativeInt(value, name) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`${name} must be a non-negative integer`);
  }
  return parsed;
}

function parseDelayMs(value) {
  if (value === '0') return 0;
  const match = /^([0-9]+)(ns|us|ms|s)$/.exec(value);
  if (!match) throw new Error(`invalid delay: ${value}`);
  const amount = Number.parseInt(match[1], 10);
  switch (match[2]) {
    case 'ns':
    case 'us':
      return 0;
    case 'ms':
      return amount;
    case 's':
      return amount * 1000;
    default:
      throw new Error(`invalid delay: ${value}`);
  }
}

function sleep(ms) {
  if (ms <= 0) return Promise.resolve();
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  mkdirSync(dirname(args.path), { recursive: true });
  mkdirSync(dirname(args.readyFile), { recursive: true });

  const lock = WriterLock.acquire(args.path);
  let writer;

  try {
    writer = Writer.create(args.path, writerOptionsFromArgs(args));
    await appendLiveEntries(writer, args);
    writer.close();
    lock.release();
  } catch (error) {
    if (writer) {
      try {
        writer.close();
      } catch {
        // Preserve the original failure.
      }
    }
    try {
      lock.release();
    } catch {
      // Preserve the original failure.
    }
    throw error;
  }
}

function writerOptionsFromArgs(args) {
  const writerOptions = {
    compression: args.compression,
    compressionThresholdBytes: args.compressionThresholdBytes,
    compact: args.compact,
  };
  if (args.seal) {
    writerOptions.seal = new SealOptions(
      Buffer.alloc(12),
      args.sealIntervalUsec,
      args.sealStartUsec,
    );
  }
  return writerOptions;
}

async function appendLiveEntries(writer, args) {
  const realtimeBase = 1_700_001_000_000_000n;
  try {
    for (let i = 0; i < args.entries; i++) {
      writer.append(liveFieldsForEntry(args, i), {
        realtimeUsec: realtimeBase + BigInt(i),
        monotonicUsec: BigInt(i + 1),
      });
      await handleLiveAppendSideEffects(writer, args, i);
    }
  } catch (error) {
    throw error;
  }
}

async function handleLiveAppendSideEffects(writer, args, index) {
  if (index === 0) {
    writer.sync();
    writeFileSync(args.readyFile, 'ready\n', { mode: 0o600 });
  } else if (args.syncEvery > 0 && (index + 1) % args.syncEvery === 0) {
    writer.sync();
  }
  if (args.crashAfter > 0 && index + 1 >= args.crashAfter) process.exit(17);
  await sleep(args.delayMs);
}

function liveFieldsForEntry(args, index) {
  if (args.binaryFixture && index === 0) return binaryFixtureFields();
  if (args.zstdFixture && index === 0) return compressedFixtureFields('zstd');
  if (args.lz4Fixture && index === 0) return compressedFixtureFields('lz4');
  if (args.xzFixture && index === 0) return compressedFixtureFields('xz');
  return defaultLiveFields(index);
}

function binaryFixtureFields() {
  return [
    { name: 'TEST_ID', value: 'binary-interoperability' },
    { name: 'MESSAGE', value: 'binary interoperability' },
    { name: 'PRIORITY', value: '6' },
    { name: 'LIVE_SEQ', value: '000000' },
    { name: 'BINARY_PAYLOAD', value: Buffer.from([0x00, 0x01, 0x02, 0x41, 0x0a, 0x7f, 0x80, 0xff]) },
    { name: 'BINARY_MATCH', value: Buffer.from([0x61, 0x62, 0x63, 0x07, 0x64, 0x65, 0x66]) },
    { name: 'BINARY_EMPTY', value: Buffer.from([]) },
    { name: 'BINARY_COMPRESSIBLE', value: Buffer.alloc(256, 0x41) },
  ];
}

function compressedFixtureFields(kind) {
  const largePayload = patternedPayload();
  return [
    { name: 'TEST_ID', value: `${kind}-interoperability` },
    { name: 'MESSAGE', value: `${kind} interoperability` },
    { name: 'PRIORITY', value: '6' },
    { name: 'LIVE_SEQ', value: '000000' },
    { name: 'COMPRESSED_PAYLOAD', value: largePayload },
    { name: 'COMPRESSED_MATCH', value: largePayload.subarray(0, 32) },
  ];
}

function patternedPayload() {
  const largePayload = Buffer.alloc(256);
  for (let j = 0; j < largePayload.length; j++) largePayload[j] = (j % 26) + 0x41;
  return largePayload;
}

function defaultLiveFields(index) {
  return [
    { name: 'MESSAGE', value: `live-${index.toString().padStart(6, '0')}` },
    { name: 'PRIORITY', value: '6' },
    { name: 'SYSLOG_IDENTIFIER', value: 'node-live-writer' },
    { name: 'LIVE_SEQ', value: index.toString().padStart(6, '0') },
  ];
}

main().catch((error) => {
  process.stderr.write(`${error.message}\n`);
  process.exit(1);
});
