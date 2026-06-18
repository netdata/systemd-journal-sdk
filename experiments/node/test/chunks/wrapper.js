import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { Writer } from '../../src/lib/writer.js';

const here = dirname(fileURLToPath(import.meta.url));
const nodeRoot = resolve(here, '..', '..');
const wrapperPath = resolve(nodeRoot, 'cmd/netdata_function_wrapper.js');
const BASE_REALTIME_SECONDS = Number.parseInt('1700000000', 10);
const FAR_FUTURE_SECONDS = Number.parseInt('9999999999', 10);

let tmpDir = null;

function setup() {
  tmpDir = mkdtempSync(join(tmpdir(), 'wrapper-test-'));
}

function teardown() {
  if (tmpDir) { try { rmSync(tmpDir, { recursive: true }); } catch {} tmpDir = null; }
}

function buildJournal(dir) {
  const journalDir = join(dir, 'aabbccdd111111111111111111111111', 'system.journal');
  mkdirSync(join(dir, 'aabbccdd111111111111111111111111'), { recursive: true });
  const writer = Writer.create(journalDir, {
    machineId: Buffer.from('aabbccdd11111111aabbccdd11111111', 'hex'),
    bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
    seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
  });
  for (let i = 0; i < 10; i++) {
    writer.append([
      { name: 'MESSAGE', value: Buffer.from(`wrapper-test-${String(i).padStart(5, '0')}`, 'utf8') },
      { name: 'PRIORITY', value: Buffer.from(String(5 + (i % 3)), 'utf8') },
    ], { realtimeUsec: (BASE_REALTIME_SECONDS + i) * 1_000_000 });
  }
  writer.close();
  return journalDir;
}

function runWrapper(args, stdinPayload, timeoutMs = 30000) {
  const cmdArgs = [
    wrapperPath,
    '--test', 'systemd-journal',
    '--dir', args.dir,
    '--timeout', String(args.timeout ?? 0),
  ];
  if (args.progressJsonl) {
    cmdArgs.push('--progress-jsonl', args.progressJsonl);
  }
  if (args.cancelImmediately) {
    cmdArgs.push('--cancel-immediately');
  }
  if (args.cancelAfterProgress != null) {
    cmdArgs.push('--cancel-after-progress', String(args.cancelAfterProgress));
  }
  const result = spawnSync('node', cmdArgs, {
    input: stdinPayload,
    encoding: 'utf8',
    timeout: timeoutMs,
  });
  return {
    exitCode: result.status,
    stdout: (result.stdout || '').trim(),
    stderr: (result.stderr || '').trim(),
    error: result.error,
  };
}

function testEnvelopeKeysPresent() {
  buildJournal(tmpDir);
  const infoRequest = JSON.stringify({ info: true });
  const result = runWrapper({ dir: tmpDir }, infoRequest);
  assert.equal(result.exitCode, 0, `exit code ${result.exitCode}, stderr: ${result.stderr}`);
  const response = JSON.parse(result.stdout);
  assert.equal(response.status, 200);
  assert.equal(response.type, 'table');
  assert.ok(response.accepted_params);
  assert.equal(response.versions.netdata_function_api, 1);
}

function testRejectsWrongFunction() {
  const result = spawnSync('node', [
    wrapperPath, '--test', 'wrong-function', '--dir', tmpDir,
  ], { input: '{}', encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.ok(result.stderr.includes('unsupported function'));
}

function testRequiresDir() {
  const result = spawnSync('node', [
    wrapperPath, '--test', 'systemd-journal',
  ], { input: '{}', encoding: 'utf8' });
  assert.notEqual(result.status, 0);
}

function testProgressJsonlWritten() {
  const progressPath = join(tmpDir, 'progress.jsonl');
  buildJournal(tmpDir);
  const dataRequest = JSON.stringify({ after: 0, before: 9999999999, data_only: true });
  const result = runWrapper({ dir: tmpDir, progressJsonl: progressPath }, dataRequest);
  assert.equal(result.exitCode, 0, `exit code ${result.exitCode}, stderr: ${result.stderr}`);
  assert.ok(existsSync(progressPath), 'progress file must exist');
  const lines = readFileSync(progressPath, 'utf8').trim().split('\n').filter(l => l);
  assert.ok(lines.length > 0, 'must have at least one progress line');
  for (const line of lines) {
    const entry = JSON.parse(line);
    assert.ok('current_file' in entry, `missing current_file in ${line}`);
    assert.ok('total_files' in entry, `missing total_files in ${line}`);
    assert.ok('matched_files' in entry, `missing matched_files in ${line}`);
    assert.ok('skipped_files' in entry, `missing skipped_files in ${line}`);
    assert.ok('elapsed_seconds' in entry, `missing elapsed_seconds in ${line}`);
    assert.ok('stats' in entry, `missing stats in ${line}`);
    assert.equal(typeof entry.current_file, 'number');
    assert.equal(typeof entry.total_files, 'number');
    assert.equal(typeof entry.matched_files, 'number');
    assert.equal(typeof entry.skipped_files, 'number');
    assert.equal(typeof entry.elapsed_seconds, 'number');
    assert.equal(typeof entry.stats, 'object');
  }
}

function testCancelImmediately() {
  buildJournal(tmpDir);
  const dataRequest = JSON.stringify({ after: 0, before: 9999999999, data_only: true });
  const result = runWrapper({ dir: tmpDir, cancelImmediately: true }, dataRequest);
  // Cancel-immediately: the function runs but is cancelled before any file
  // is processed. The SDK returns a 499 error, and the wrapper exits 0
  // because the request was handled (just cancelled).
  assert.equal(result.exitCode, 0, `exit code ${result.exitCode}, stderr: ${result.stderr}`);
  const response = JSON.parse(result.stdout);
  assert.equal(response.status, 499, `expected 499, got ${JSON.stringify(response)}`);
  assert.ok(String(response.errorMessage || '').toLowerCase().includes('cancel'),
    `errorMessage should mention cancel: ${response.errorMessage}`);
}

function testCancelAfterProgress() {
  const progressPath = join(tmpDir, 'progress.jsonl');
  buildJournal(tmpDir);

  // Write a second journal so cancel-after-progress=1 cuts between files.
  const journalDir2 = join(tmpDir, 'bbbbcccc222222222222222222222222', 'system.journal');
  mkdirSync(join(tmpDir, 'bbbbcccc222222222222222222222222'), { recursive: true });
  const w2 = Writer.create(journalDir2, {
    machineId: Buffer.from('bbbbcccc22222222bbbbcccc22222222', 'hex'),
    bootId: Buffer.from('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'hex'),
    seqnumId: Buffer.from('44444444444444444444444444444444', 'hex'),
  });
  for (let i = 0; i < 5; i++) {
    w2.append([
      { name: 'MESSAGE', value: Buffer.from(`second-file-${String(i).padStart(5, '0')}`, 'utf8') },
      { name: 'PRIORITY', value: Buffer.from(String(5), 'utf8') },
    ], { realtimeUsec: (BASE_REALTIME_SECONDS + 20 + i) * 1_000_000 });
  }
  w2.close();

  const dataRequest = JSON.stringify({ after: 0, before: FAR_FUTURE_SECONDS, data_only: true });
  const result = runWrapper(
    { dir: tmpDir, progressJsonl: progressPath, cancelAfterProgress: 1 },
    dataRequest,
  );
  assert.equal(result.exitCode, 0, `exit code ${result.exitCode}, stderr: ${result.stderr}`);
  // With cancel-after-progress=1 and two files, the first progress report
  // triggers cancellation; the second file is skipped and the response is 499.
  assert.ok(existsSync(progressPath), 'progress file must exist');
  const lines = readFileSync(progressPath, 'utf8').trim().split('\n').filter(l => l);
  assert.ok(lines.length >= 1, 'must have at least one progress line');
  const response = JSON.parse(result.stdout);
  assert.equal(response.status, 499, `expected 499, got ${JSON.stringify(response)}`);
}

function testProgressFileCreateError() {
  // Progress file path points to a directory (open for write will fail).
  const progressPath = join(tmpDir, 'readonly-dir');
  mkdirSync(progressPath, { recursive: true });
  buildJournal(tmpDir);
  const dataRequest = JSON.stringify({ after: 0, before: 9999999999, data_only: true });
  const result = runWrapper({ dir: tmpDir, progressJsonl: progressPath }, dataRequest);
  // The wrapper propagates the file-open error (matching Rust/Python behavior).
  assert.equal(result.exitCode, 1, `exit code ${result.exitCode}, stdout: ${result.stdout}`);
  assert.ok(result.stderr.length > 0,
    `stderr should contain error message, got empty`);
}

function testInvalidJsonRequest() {
  buildJournal(tmpDir);
  const result = runWrapper({ dir: tmpDir }, 'not-json');
  assert.equal(result.exitCode, 1, `exit code ${result.exitCode}, stdout: ${result.stdout}`);
  assert.ok(result.stderr.includes('invalid Netdata function JSON') ||
    result.stderr.includes('JSON'),
    `stderr should mention JSON error, got: ${result.stderr}`);
}

function testLargeResponseNotTruncated() {
  const journalDir = join(tmpDir, 'aabbccdd111111111111111111111111', 'system.journal');
  mkdirSync(join(tmpDir, 'aabbccdd111111111111111111111111'), { recursive: true });
  const writer = Writer.create(journalDir, {
    machineId: Buffer.from('aabbccdd11111111aabbccdd11111111', 'hex'),
    bootId: Buffer.from('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'hex'),
    seqnumId: Buffer.from('33333333333333333333333333333333', 'hex'),
  });
  const numRows = 800;
  const padding = 'x'.repeat(80);
  const nowUsec = Math.floor(Date.now() / 1000) * 1_000_000;
  for (let i = 0; i < numRows; i++) {
    writer.append([
      { name: 'MESSAGE', value: Buffer.from(`large-response-msg-${String(i).padStart(5, '0')}-${padding}`, 'utf8') },
      { name: 'PRIORITY', value: Buffer.from(String(5 + (i % 3)), 'utf8') },
      { name: 'SYSLOG_IDENTIFIER', value: Buffer.from(`large-resp-${String(i % 10).padStart(3, '0')}`, 'utf8') },
      { name: 'UNIT', value: Buffer.from(`test-unit-${String(i % 5).padStart(3, '0')}.service`, 'utf8') },
      { name: '_HOSTNAME', value: Buffer.from(`host-${String(i % 3).padStart(3, '0')}`, 'utf8') },
    ], { realtimeUsec: BigInt(nowUsec - 1800_000_000 + i * 100_000) });
  }
  writer.close();

  const dataRequest = JSON.stringify({ after: 0, before: 0, last: numRows });
  const result = runWrapper({ dir: tmpDir }, dataRequest);
  assert.equal(result.exitCode, 0, `exit code ${result.exitCode}, stderr: ${result.stderr}`);

  const rawBytes = Buffer.byteLength(result.stdout, 'utf8');
  assert.ok(rawBytes > 65536, `response must exceed 64 KiB for drain test, got ${rawBytes} bytes`);

  let response;
  try {
    response = JSON.parse(result.stdout);
  } catch (e) {
    assert.fail(`stdout is not valid JSON (likely truncated): ${e.message}. First 200 chars: ${result.stdout.slice(0, 200)}`);
  }
  assert.equal(response.status, 200, `expected 200, got ${response.status}`);
  assert.ok(Array.isArray(response.data), 'response must have data array');
  assert.equal(response.data.length, numRows, `expected ${numRows} rows, got ${response.data.length}`);
}

export async function run() {
  setup();
  try { testEnvelopeKeysPresent(); } finally { teardown(); }
  setup();
  try { testRejectsWrongFunction(); } finally { teardown(); }
  setup();
  try { testRequiresDir(); } finally { teardown(); }
  setup();
  try { testProgressJsonlWritten(); } finally { teardown(); }
  setup();
  try { testCancelImmediately(); } finally { teardown(); }
  setup();
  try { testCancelAfterProgress(); } finally { teardown(); }
  setup();
  try { testProgressFileCreateError(); } finally { teardown(); }
  setup();
  try { testInvalidJsonRequest(); } finally { teardown(); }
  setup();
  try { testLargeResponseNotTruncated(); } finally { teardown(); }
  console.log('  PASS netdata function wrapper');
}
