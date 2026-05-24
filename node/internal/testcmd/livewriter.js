#!/usr/bin/env node

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { Writer } from '../../src/lib/writer.js';

function parseArgs(argv) {
  const args = {
    path: '',
    readyFile: '',
    entries: 1000,
    delayMs: 1,
    syncEvery: 25,
    crashAfter: 0,
  };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) throw new Error(`${arg} requires a value`);
      return argv[++i];
    };
    switch (arg) {
      case '--path':
        args.path = next();
        break;
      case '--ready-file':
        args.readyFile = next();
        break;
      case '--entries':
        args.entries = parsePositiveInt(next(), '--entries');
        break;
      case '--delay':
        args.delayMs = parseDelayMs(next());
        break;
      case '--sync-every':
        args.syncEvery = parseNonNegativeInt(next(), '--sync-every');
        break;
      case '--crash-after':
        args.crashAfter = parseNonNegativeInt(next(), '--crash-after');
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }

  if (!args.path || !args.readyFile || args.entries <= 0) {
    throw new Error('path, ready-file, and positive entries are required');
  }
  return args;
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

  const writer = Writer.create(args.path);
  const realtimeBase = 1_700_001_000_000_000n;

  try {
    for (let i = 0; i < args.entries; i++) {
      writer.append([
        { name: 'MESSAGE', value: `live-${i.toString().padStart(6, '0')}` },
        { name: 'PRIORITY', value: '6' },
        { name: 'SYSLOG_IDENTIFIER', value: 'node-live-writer' },
        { name: 'LIVE_SEQ', value: i.toString().padStart(6, '0') },
      ], {
        realtimeUsec: realtimeBase + BigInt(i),
        monotonicUsec: BigInt(i + 1),
      });

      if (i === 0) {
        writer.sync();
        writeFileSync(args.readyFile, 'ready\n', { mode: 0o600 });
      } else if (args.syncEvery > 0 && (i + 1) % args.syncEvery === 0) {
        writer.sync();
      }

      if (args.crashAfter > 0 && i + 1 >= args.crashAfter) {
        process.exit(17);
      }

      await sleep(args.delayMs);
    }

    writer.close();
  } catch (error) {
    try {
      writer.close();
    } catch {
      // Preserve the original failure.
    }
    throw error;
  }
}

main().catch((error) => {
  process.stderr.write(`${error.message}\n`);
  process.exit(1);
});
