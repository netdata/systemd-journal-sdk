#!/usr/bin/env node
// SPDX-License-Identifier: MIT-0

import { parseArgs } from 'node:util';
import { openSync, closeSync, writeSync } from 'node:fs';
import { Buffer } from 'node:buffer';

import {
  NetdataFunctionRunOptions,
  NetdataJournalFunction,
} from '../src/lib/netdata.js';

const FUNCTION_NAME = 'systemd-journal';

// ---------------------------------------------------------------------------
// Lightweight arg parsing via Node 22 util.parseArgs.
// Hyphenated flags become camelCase keys.
// ---------------------------------------------------------------------------

const { values: args, positionals } = parseArgs({
  options: {
    test: { type: 'string' },
    dir: { type: 'string' },
    timeout: { type: 'string', default: '0' },
    'progress-jsonl': { type: 'string' },
    'cancel-immediately': { type: 'boolean', default: false },
    'cancel-after-progress': { type: 'string', default: '0' },
  },
  allowPositionals: true,
});

if (args.test !== FUNCTION_NAME) {
  process.stderr.write(`unsupported function '${args.test}'\n`);
  process.exit(1);
}

if (!args.dir) {
  process.stderr.write('--dir is required\n');
  process.exit(1);
}

const timeoutSeconds = parseInt(args.timeout || '0', 10);
const cancelImmediately = args['cancel-immediately'] === true;
const cancelAfterProgress = parseInt(args['cancel-after-progress'] || '0', 10);
const progressPath = args['progress-jsonl'] || null;

// ---------------------------------------------------------------------------
// ProgressRecorder — mirrors the Python/Rust ProgressRecorder pattern.
// ---------------------------------------------------------------------------

class ProgressRecorder {
  constructor(path, cancelImmediately, cancelAfterProgress) {
    this._cancelled = cancelImmediately;
    this._reports = 0;
    this._cancelAfterProgress = cancelAfterProgress;
    this._fd = null;
    this._writeError = null;
    if (path != null) {
      this._fd = openSync(path, 'w');
    }
  }

  close() {
    if (this._fd != null) {
      try { closeSync(this._fd); } catch {}
      this._fd = null;
    }
  }

  handle(progress) {
    if (this._writeError != null) return;
    this._reports += 1;
    if (this._fd != null) {
      const line = {
        current_file: Number(progress.currentFile),
        total_files: Number(progress.totalFiles),
        matched_files: Number(progress.matchedFiles),
        skipped_files: Number(progress.skippedFiles),
        elapsed_seconds: Number(progress.elapsed),
        stats: statsToJsonable(progress.stats),
      };
      try {
        writeSync(this._fd, JSON.stringify(line) + '\n');
      } catch (err) {
        this._writeError = `failed to write progress JSON: ${err.message}`;
        this._cancelled = true;
      }
    }
    if (this._cancelAfterProgress > 0 && this._reports >= this._cancelAfterProgress) {
      this._cancelled = true;
    }
  }

  isCancelled() {
    return this._cancelled;
  }

  takeWriteError() {
    return this._writeError;
  }
}

function statsToJsonable(stats) {
  if (stats == null) return {};
  if (typeof stats.toJson === 'function') return stats.toJson();
  const out = {};
  for (const [k, v] of Object.entries(stats)) {
    if (typeof v === 'bigint') out[k] = Number(v);
    else out[k] = v;
  }
  return out;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

async function main() {
  const requestBytes = await readStdin();

  const recorder = new ProgressRecorder(progressPath, cancelImmediately, cancelAfterProgress);

  let exitCode = 0;
  try {
    const options = NetdataFunctionRunOptions.fromTimeoutSeconds(timeoutSeconds);
    if (recorder._fd != null || cancelAfterProgress > 0) {
      options.progressCallback = (p) => recorder.handle(p);
    }
    if (cancelImmediately || cancelAfterProgress > 0) {
      options.cancellationCallback = () => recorder.isCancelled();
    }

    const response = NetdataJournalFunction.systemdJournalPluginCompatible()
      .runDirectoryRequestBytesWithOptions(args.dir, requestBytes, options);

    const writeError = recorder.takeWriteError();
    if (writeError != null) {
      process.stderr.write(writeError + '\n');
      exitCode = 1;
    } else {
      process.stdout.write(JSON.stringify(response) + '\n');
    }
  } catch (err) {
    if (err instanceof Error) {
      process.stderr.write(`${err.message}\n`);
    } else {
      process.stderr.write(`${err}\n`);
    }
    exitCode = 1;
  } finally {
    recorder.close();
  }
  process.exit(exitCode);
}

main();
