import { createReadStream, createWriteStream, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { createZstdDecompress } from 'node:zlib';
import { isMainThread, parentPort, workerData, Worker } from 'node:worker_threads';

const STATUS_PENDING = 0;
const STATUS_OK = 1;
const STATUS_ERROR = 2;
const DEFAULT_ZSTD_TIMEOUT_MS = 600000;
const MAX_ERROR_BYTES = 4096;

export class ZstdStreamingError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ZstdStreamingError';
  }
}

export function streamZstToTempSync(inputPath, options = {}) {
  const timeoutMs = normalizeTimeout(options.timeoutMs ?? DEFAULT_ZSTD_TIMEOUT_MS);
  const prefix = options.prefix ?? 'node-sdk-journal';
  const tempDir = mkdtempSync(join(tmpdir(), `${prefix}-`));
  const outputPath = join(tempDir, 'decompressed.journal');
  const errorPath = join(tempDir, 'error.txt');
  const statusBuffer = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT);
  const status = new Int32Array(statusBuffer);

  const cleanup = () => {
    rmSync(tempDir, { recursive: true, force: true });
  };

  let worker;
  try {
    worker = new Worker(new URL(import.meta.url), {
      workerData: {
        systemdJournalZstWorker: true,
        inputPath,
        outputPath,
        errorPath,
        statusBuffer,
      },
    });
  } catch (error) {
    cleanup();
    throw new ZstdStreamingError(`zstd worker startup failed: ${sanitizeError(error)}`);
  }

  const waitResult = Atomics.wait(status, 0, STATUS_PENDING, timeoutMs);
  if (waitResult === 'timed-out') {
    worker.terminate().catch(() => {});
    cleanup();
    throw new ZstdStreamingError('zstd worker timed out');
  }
  const code = Atomics.load(status, 0);
  if (code !== STATUS_OK) {
    worker.terminate().catch(() => {});
    const message = readWorkerError(errorPath);
    cleanup();
    throw new ZstdStreamingError(message || 'zstd worker failed');
  }

  worker.terminate().catch(() => {});
  return { path: outputPath, cleanup };
}

function normalizeTimeout(value) {
  const timeout = Number(value);
  if (!Number.isSafeInteger(timeout) || timeout <= 0) {
    throw new ZstdStreamingError('zstd timeout must be a positive safe integer');
  }
  return timeout;
}

function readWorkerError(path) {
  try {
    return readFileSync(path, 'utf8').slice(0, MAX_ERROR_BYTES);
  } catch {
    return '';
  }
}

function sanitizeError(error) {
  const message = error && error.message ? String(error.message) : String(error);
  return message.replaceAll('\n', ' ').slice(0, MAX_ERROR_BYTES);
}

async function runWorker() {
  const status = new Int32Array(workerData.statusBuffer);
  try {
    await pipeline(
      createReadStream(workerData.inputPath),
      createZstdDecompress(),
      createWriteStream(workerData.outputPath, { flags: 'wx', mode: 0o600 }),
    );
    Atomics.store(status, 0, STATUS_OK);
    Atomics.notify(status, 0);
  } catch (error) {
    try {
      writeFileSync(workerData.errorPath, sanitizeError(error), { flag: 'wx', mode: 0o600 });
    } catch {
      // Preserve the original decompression failure.
    }
    Atomics.store(status, 0, STATUS_ERROR);
    Atomics.notify(status, 0);
  } finally {
    parentPort?.close();
  }
}

if (!isMainThread && workerData?.systemdJournalZstWorker) {
  runWorker();
}
