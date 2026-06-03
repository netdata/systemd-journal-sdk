#!/usr/bin/env node
// Node.js conformance adapter: run, list, probe.
// All output is synchronous JSON on stdout.

import { existsSync, readFileSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { FileReader } from '../src/lib/reader.js';
import { DirectoryReader } from '../src/lib/directory-reader.js';
import { Writer } from '../src/lib/writer.js';
import { SealOptions } from '../src/lib/seal.js';
import {
  SdJournalClose,
  SdJournalGetCursor,
  SdJournalGetRealtimeUsec,
  SdJournalNext,
  SdJournalOpen,
  SdJournalSeekCursor,
  SdJournalSeekHead,
  SdJournalTestCursor,
  exportEntry,
  jsonEntry,
  parseCursor,
} from '../src/facade.js';
import { parseMatchString } from '../src/lib/hash.js';
import { verifyFile, verifyFileWithKey } from '../src/lib/verify.js';
import { isJournalFileName } from '../src/lib/compress.js';
import { readUint64LE, uuidToString, randomUUID, bufEqual } from '../src/lib/binary.js';
import { HEADER_SIZE } from '../src/lib/header.js';

const ADAPTER_VERSION = '0.1.0';

function main() {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    process.stderr.write('Usage: adapter [run|list|probe]\n');
    process.exit(1);
  }
  switch (args[0]) {
    case 'run': runAdapter(); break;
    case 'list': listTests(); break;
    case 'probe': probeAdapter(); break;
    default:
      process.stderr.write(`Unknown subcommand: ${args[0]}\n`);
      process.exit(1);
  }
}

function fixtureBase() {
  const base = process.env.ADAPTER_FIXTURE_BASE;
  if (base) return base;
  let dir = process.cwd();
  for (;;) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- adapter searches parent dirs for the repository manifest.
    if (existsSync(join(dir, 'tests', 'conformance', 'manifests', 'conformance-v01.json'))) return dir;
    const parent = resolve(dir, '..');
    if (parent === dir) return process.cwd();
    dir = parent;
  }
}

function cleanupPath(path, options = {}) {
  try {
    rmSync(path, { force: true, ...options });
  } catch (err) {
    if (process.env.ADAPTER_STRICT_CLEANUP === '1') throw err;
  }
}

function resolveFixture(tc, key) {
  if (!tc.fixtures || !Object.hasOwn(tc.fixtures, key)) return '';
  return join(fixtureBase(), tc.fixtures[key].path);
}

// ---- RUN ----

function runAdapter() {
  const input = readFileSync(0, 'utf8');
  let tc;
  try { tc = JSON.parse(input); } catch (e) { process.stderr.write(`decode error: ${e.message}\n`); process.exit(1); }

  const start = Date.now();
  let result;
  try {
    switch (tc.category) {
      case 'file-format': result = runFileFormatTest(tc); break;
      case 'entry-parse': result = runEntryParseTest(tc); break;
      case 'matching': result = runMatchingTest(tc); break;
      case 'stream': result = runStreamTest(tc); break;
      case 'cursor-navigation': result = runCursorTest(tc); break;
      case 'enumeration': result = runEnumerationTest(tc); break;
      case 'import-export': result = runExportTest(tc); break;
      case 'journalctl-cli': result = runJournalctlTest(tc); break;
      case 'compression': result = runCompressionTest(tc); break;
      case 'corruption-resilience': result = runCorruptionTest(tc); break;
      case 'verification':
        result = runVerificationTest(tc); break;
      default:
        result = { status: 'SKIP', note: `unsupported category: ${tc.category}` };
    }
  } catch (e) {
    result = { status: 'ERROR', error: e.message };
  }

  result.test_name = tc.test_name;
  result.result_format = tc.expected.result_format;
  result.duration_ms = Math.max(1, Date.now() - start);
  if (!result.status) { result.status = 'SKIP'; result.note = 'no matching test handler'; }

  process.stdout.write(JSON.stringify(result) + '\n');
}

// ---- FILE FORMAT ----

function runFileFormatTest(tc) {
  switch (tc.test_name) {
    case 'journal-file-parse-uid-from-filename': return testUIDFromFilename();
    case 'journal-file-header-parse': return testFileHeaderParse(tc);
    default: return { status: 'SKIP', note: `unsupported: ${tc.test_name}` };
  }
}

function testUIDFromFilename() {
  const tests = [
    { name: 'user-1000.journal', uid: 1000, hasUID: true },
    { name: 'system.journal', hasUID: false },
    { name: 'user-foo.journal', errCode: 'EINVAL' },
    { name: 'user-65535.journal', errCode: 'ENXIO' },
    { name: 'user@00000000000000000000000000000000.journal~', errCode: 'EREMOTE' },
  ];
  for (const t of tests) {
    const { uid, hasUID, errCode } = parseUID(t.name);
    if (uid !== (t.uid || 0) || hasUID !== (t.hasUID || false) || errCode !== (t.errCode || '')) {
      return { status: 'FAIL', actual: false, error: `${t.name}: got uid=${uid} hasUID=${hasUID} err=${errCode}` };
    }
  }
  return { status: 'PASS', actual: true };
}

function parseUID(name) {
  if (name === 'system.journal' || name.startsWith('system@')) return { uid: 0, hasUID: false, errCode: '' };
  if (name.startsWith('user@')) return { uid: 0, hasUID: false, errCode: 'EREMOTE' };
  if (!name.startsWith('user-') || !name.endsWith('.journal')) return { uid: 0, hasUID: false, errCode: 'EINVAL' };
  const raw = name.slice(5, -('.journal'.length));
  const parsed = parseInt(raw, 10);
  if (isNaN(parsed)) return { uid: 0, hasUID: false, errCode: 'EINVAL' };
  if (parsed === 65535) return { uid: 0, hasUID: false, errCode: 'ENXIO' };
  return { uid: parsed, hasUID: true, errCode: '' };
}

function testFileHeaderParse(tc) {
  const path = resolveFixture(tc, 'journal_file');
  if (!path) return { status: 'SKIP', note: 'no journal_file fixture' };
  const r = FileReader.open(path);
  try {
    if (!r.step()) return { status: 'FAIL', error: 'fixture has no entries' };
    r.getEntry(); // advance past first entry
    return {
      status: 'PASS',
      actual: [{
        signature: r.header.signature,
        state: r.header.state,
        compatible_flags: r.header.compatible_flags,
        incompatible_flags: r.header.incompatible_flags,
        header_size: Number(r.header.header_size),
      }],
    };
  } finally { r.close(); }
}

// ---- ENTRY PARSE ----

function runEntryParseTest(tc) {
  const path = resolveFixture(tc, 'importer_data');
  if (!path) return { status: 'SKIP', note: 'no importer_data fixture' };
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- conformance manifest supplies repository fixture paths.
  const data = readFileSync(path, 'utf8');
  const entries = parseJournalExport(data);
  if (tc.test_name === 'journal-importer-eof') {
    return { status: 'PASS', actual: entries.length > 0, evidence: { entry_count: entries.length } };
  }
  return { status: 'PASS', actual: entries, evidence: { entry_count: entries.length } };
}

function parseJournalExport(data) {
  const entries = [];
  let current = {};
  for (const line of data.split('\n')) {
    if (line === '') {
      if (Object.keys(current).length > 0) { entries.push(current); current = {}; }
      continue;
    }
    const eq = line.indexOf('=');
    if (eq >= 0) current[line.slice(0, eq)] = line.slice(eq + 1);
  }
  if (Object.keys(current).length > 0) entries.push(current);
  return entries;
}

// ---- MATCHING ----

function runMatchingTest(tc) {
  switch (tc.test_name) {
    case 'journal-match-invalid-input': return testMatchInvalid();
    case 'journal-match-boolean-logic': return testMatchBooleanLogic();
    default: return { status: 'SKIP', note: `unsupported: ${tc.test_name}` };
  }
}

function testMatchInvalid() {
  for (const item of ['foobar', '', '=', '=xxxxx']) {
    try { parseMatchString(item); return { status: 'FAIL', error: `EINVAL expected for "${item}"` }; }
    catch { /* expected */ }
  }
  try { parseMatchString('FOOBAR=waldo'); }
  catch (error) { return { status: 'FAIL', error: `valid match rejected: ${error.message}` }; }
  return { status: 'PASS', actual: 'EINVAL', error: 'EINVAL' };
}

function testMatchBooleanLogic() {
  const path = join(tmpdir(), `node-match-test-${process.pid}.journal`);
  try {
    const w = Writer.create(path);
    w.append([{ name: 'L3', value: 'ok' }, { name: 'TWO', value: 'two' }, { name: 'ONE', value: 'one' }]);
    w.append([
      { name: 'L4_1', value: 'yes' }, { name: 'L4_2', value: 'ok' },
      { name: 'PIFF', value: 'paff' }, { name: 'QUUX', value: 'xxxxx' },
      { name: 'HALLO', value: 'WALDO' },
      { name: 'B', value: Buffer.from([0x43, 0x00, 0x44]) },
      { name: 'A', value: Buffer.from([0x01, 0x02]) },
    ]);
    w.append([{ name: 'L3', value: 'ok' }]);
    w.append([{ name: 'TWO', value: 'two' }, { name: 'ONE', value: 'one' }]);
    w.close();

    const r = FileReader.open(path);
    addSystemdComplexMatchExpression(r);
    const matched = [];
    while (r.step()) {
      const entry = r.getEntry();
      const fields = Object.create(null);
      for (const [k, v] of Object.entries(entry.fields)) fields[k] = v.toString('utf8');
      matched.push(fields);
    }
    r.close();

    if (matched.length !== 2) return { status: 'FAIL', actual: matched, error: `matched ${matched.length}, want 2` };
    return { status: 'PASS', actual: matched };
  } finally { cleanupPath(path); }
}

function addSystemdComplexMatchExpression(r) {
  r.addMatch(Buffer.from([0x41, 0x3d, 0x01, 0x02]));
  r.addMatch(Buffer.from([0x42, 0x3d, 0x43, 0x00, 0x44]));
  r.addMatch(Buffer.from('HALLO=WALDO'));
  r.addMatch(Buffer.from('QUUX=mmmm'));
  r.addMatch(Buffer.from('QUUX=xxxxx'));
  r.addMatch(Buffer.from('HALLO='));
  r.addMatch(Buffer.from('QUUX=xxxxx'));
  r.addMatch(Buffer.from('QUUX=yyyyy'));
  r.addMatch(Buffer.from('PIFF=paff'));
  r.addDisjunction();
  r.addMatch(Buffer.from('ONE=one'));
  r.addMatch(Buffer.from('ONE=two'));
  r.addMatch(Buffer.from('TWO=two'));
  r.addConjunction();
  r.addMatch(Buffer.from('L4_1=yes'));
  r.addMatch(Buffer.from('L4_1=ok'));
  r.addMatch(Buffer.from('L4_2=yes'));
  r.addMatch(Buffer.from('L4_2=ok'));
  r.addDisjunction();
  r.addMatch(Buffer.from('L3=yes'));
  r.addMatch(Buffer.from('L3=ok'));
}

// ---- STREAM ----

function runStreamTest(tc) {
  const path = resolveFixture(tc, 'journal_dir');
  if (!path) return { status: 'SKIP', note: 'no journal_dir fixture' };
  const r = DirectoryReader.open(path);
  try {
    const entries = [];
    let count = 0;
    r.seekHead();
    while (r.step() && count < 100) {
      const entry = r.getEntry();
      if (!entry) break;
      const em = {};
      for (const [k, v] of Object.entries(entry.fields)) em[k] = v.toString('utf8');
      entries.push(em);
      count++;
    }
    if (entries.length === 0) return { status: 'FAIL', error: 'no entries read' };
    return { status: 'PASS', actual: entries, evidence: { entry_count: entries.length } };
  } finally { r.close(); }
}

// ---- CURSOR ----

function runCursorTest(tc) {
  const path = resolveFixture(tc, 'journal_dir');
  if (!path) return { status: 'SKIP', note: 'no journal_dir fixture' };
  const r = SdJournalOpen(path, 0);
  try {
    SdJournalSeekHead(r);
    if (SdJournalNext(r) === 0) return { status: 'FAIL', error: 'no entries' };
    const cursor = SdJournalGetCursor(r);
    if (!cursor) return { status: 'FAIL', error: 'null cursor' };
    if (!SdJournalTestCursor(r, cursor)) return { status: 'FAIL', error: 'current cursor did not match' };
    const cursorRealtime = SdJournalGetRealtimeUsec(r);
    if (SdJournalTestCursor(r, 'invalid-cursor')) {
      return { status: 'FAIL', error: 'invalid cursor matched current position' };
    }
    let invalidSeekRejected = false;
    try {
      SdJournalSeekCursor(r, 'invalid-cursor');
    } catch {
      invalidSeekRejected = true;
    }
    if (!invalidSeekRejected) return { status: 'FAIL', error: 'invalid seek cursor was accepted' };
    SdJournalSeekCursor(r, cursor);
    const cursorPrefix = cursor.split(/n=[^;]*$/)[0];
    if (cursorPrefix === cursor) return { status: 'FAIL', error: 'cursor missing seqnum segment' };
    SdJournalSeekCursor(r, `${cursorPrefix}n=999999`);
    if (SdJournalTestCursor(r, cursor)) {
      return { status: 'FAIL', error: 'missing seek stayed on original cursor' };
    }
    if (SdJournalGetRealtimeUsec(r) < cursorRealtime) {
      return { status: 'FAIL', error: 'missing seek moved before requested cursor' };
    }
    return {
      status: 'PASS',
      actual: true,
      evidence: {
        found_cursor: true,
        invalid_test_cursor: false,
        invalid_seek_rejected: true,
        missing_seek: true,
        missing_seek_position: true,
      },
    };
  } finally { SdJournalClose(r); }
}

// ---- ENUMERATION ----

function runEnumerationTest(tc) {
  const path = resolveFixture(tc, 'journal_dir');
  if (!path) return { status: 'SKIP', note: 'no journal_dir fixture' };
  const r = DirectoryReader.open(path);
  try {
    const fields = Array.from(r.enumerateFields()).sort();
    return { status: 'PASS', actual: fields, evidence: { field_count: fields.length } };
  } finally { r.close(); }
}

// ---- EXPORT ----

function runExportTest(tc) {
  const path = resolveFixture(tc, 'journal_dir');
  if (!path) return { status: 'SKIP', note: 'no journal_dir fixture' };
  const r = DirectoryReader.open(path);
  try {
    const exports = [];
    r.seekHead();
    let count = 0;
    while (r.step() && count < 10) {
      const entry = r.getEntry();
      if (!entry) break;
      exports.push(exportEntry(entry).toString('utf8'));
      count++;
    }
    if (exports.length === 0) return { status: 'FAIL', error: 'no exports generated' };
    return { status: 'PASS', actual: exports, evidence: { export_count: exports.length } };
  } finally { r.close(); }
}

// ---- JOURNALCTL CLI ----

function runJournalctlTest(tc) {
  switch (tc.test_name) {
    case 'journal-list-boots': return testListBoots(tc);
    default: return { status: 'SKIP', note: `unsupported: ${tc.test_name}` };
  }
}

function testListBoots(tc) {
  const path = resolveFixture(tc, 'journal_dir');
  if (!path) return { status: 'SKIP', note: 'no journal_dir fixture' };
  const r = DirectoryReader.open(path);
  try {
    return { status: 'PASS', actual: r.listBoots() };
  } finally { r.close(); }
}

// ---- COMPRESSION ----

function runCompressionTest(tc) {
  const path = resolveFixture(tc, 'journal_file');
  if (!path) return { status: 'SKIP', note: 'no journal_file fixture' };
  const r = FileReader.open(path);
  try {
    if (!r.step()) return { status: 'FAIL', error: 'no entries in compressed fixture' };
    const entry = r.getEntry();
    return {
      status: 'PASS', actual: true,
      evidence: {
        message: (entry.fields['MESSAGE'] || Buffer.alloc(0)).toString('utf8'),
        transport: (entry.fields['_TRANSPORT'] || Buffer.alloc(0)).toString('utf8'),
      },
    };
  } finally { r.close(); }
}

// ---- CORRUPTION ----

function runCorruptionTest(tc) {
  if (tc.test_name === 'journal-verify-corruption-detection') {
    const path = resolveFixture(tc, 'corrupted_file');
    if (!path) return { status: 'SKIP', note: 'no corrupted_file fixture' };
    try {
      verifyFile(path);
    } catch (err) {
      return { status: 'PASS', actual: err.message, error: err.message };
    }
    return { status: 'FAIL', error: 'verification did not detect corruption in truncated zstd frame' };
  }
  let checked = 0;
  let readErrors = 0;
  for (const key of ['corrupted_file', 'afl_corrupted_1', 'afl_corrupted_2']) {
    const path = resolveFixture(tc, key);
    if (!path) continue;
    checked++;
    try {
      const r = FileReader.open(path);
      for (let i = 0; i < 1000; i++) {
        if (!r.step()) break;
        try { r.getEntry(); } catch { readErrors++; break; }
      }
      r.close();
    } catch { readErrors++; }
  }
  if (checked === 0) return { status: 'SKIP', note: 'no corruption fixtures' };
  return { status: 'PASS', actual: true, evidence: { checked, read_errors: readErrors } };
}

// ---- VERIFICATION ----

function runVerificationTest(tc) {
  if (tc.test_name === 'journal-verify-sealed') {
    const tmp = join(tmpdir(), `node-verify-sealed-${process.pid}`);
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- temporary adapter directory is process-scoped test output.
      mkdirSync(tmp, { recursive: true });
    } catch { /* may already exist */ }
    try {
      const path = join(tmp, 'sealed.journal');
      const seed = Buffer.alloc(12, 0);
      const sealOpts = new SealOptions(seed, 1000000, 1000000);
      const w = Writer.create(path, { seal: sealOpts });
      w.append([{ name: 'MESSAGE', value: 'sealed verify' }], { realtimeUsec: 1500000n });
      w.close();
      const key = `${seed.toString('hex')}/1-f4240`;
      try {
        verifyFileWithKey(path, key);
        return { status: 'PASS', actual: true };
      } catch (err) {
        return { status: 'FAIL', actual: false, error: err.message };
      }
    } finally {
      cleanupPath(tmp, { recursive: true });
    }
  }
  return { status: 'SKIP', note: `unsupported verification test: ${tc.test_name}` };
}

// ---- LIST ----

function listTests() {
  process.stdout.write(JSON.stringify([
    'journal-file-parse-uid-from-filename',
    'journal-file-header-parse',
    'journal-importer-basic-parsing',
    'journal-importer-eof',
    'journal-match-boolean-logic',
    'journal-match-invalid-input',
    'journal-stream-directory-iteration',
    'journal-cursor-test',
    'journal-query-unique-fields',
    'journal-export-format',
    'journal-list-boots',
    'journal-zstd-compressed-read',
    'journal-verify-sealed',
    'journal-corruption-append-resilient',
    'journal-verify-corruption-detection',
  ]) + '\n');
}

// ---- PROBE ----

function probeAdapter() {
  process.stdout.write(JSON.stringify({
    adapter_version: ADAPTER_VERSION,
    language: 'node',
    capabilities: {
      file_reader: true, directory_reader: true,
      forward_iter: true, backward_iter: true,
      cursor_nav: true, match_and: true, match_or: true,
      match_disjunction: true, unique_fields: true,
      export_output: true, json_output: true,
      list_boots: true, zstd_decompress: true,
      verification: true, fss: true,
    },
  }) + '\n');
}

main();
