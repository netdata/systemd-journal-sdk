// Compression support and journal file name helpers.

import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { zstdDecompressSync } from 'node:zlib';

// Decompress zstd bytes or file path to a Buffer.
export function decompressZstSync(input) {
  const src = Buffer.isBuffer(input) ? input : readFileSync(input);
  return zstdDecompressSync(src);
}

// Decompress a zstd file to a temp file and return the path.
export function decompressZstToTemp(inputPath, prefix = 'node-journal') {
  const tempDir = mkdtempSync(join(tmpdir(), `${prefix}-`));
  const tempPath = join(tempDir, 'decompressed.journal');
  writeFileSync(tempPath, decompressZstSync(inputPath), { flag: 'wx', mode: 0o600 });
  return tempPath;
}

// Check if a filename is a recognized journal file variant.
export function isJournalFileName(name) {
  return name.endsWith('.journal') ||
         name.endsWith('.journal~') ||
         name.endsWith('.journal.zst') ||
         name.endsWith('.journal~.zst');
}

// Check if a path ends with .zst.
export function isZstFile(path) {
  return path.endsWith('.zst');
}
