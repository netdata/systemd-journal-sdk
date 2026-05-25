#!/usr/bin/env node
// Pure-JavaScript journalctl for file-backed/query behavior.

import { writeSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import {
  SdJournalOpen, SdJournalAddMatch, SdJournalAddDisjunction,
  SdJournalListBoots, SdJournalEnumerateFields, SdJournalSeekHead,
  SdJournalNext, SdJournalGetEntry, SdJournalProcessOutput,
  SdJournalSeekTail, SdJournalPrevious,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from '../../src/facade.js';
import { verifyFile, verifyFileWithKey } from '../../src/lib/verify.js';
import { FileReader } from '../../src/lib/reader.js';
import { isJournalFileName } from '../../src/lib/compress.js';
import { COMPATIBLE_SEALED } from '../../src/lib/header.js';

let parsed;
try {
  parsed = parseArgs({
    options: {
      file: { type: 'string', short: 'f' },
      directory: { type: 'string', short: 'd' },
      output: { type: 'string', default: 'default' },
      'list-boots': { type: 'boolean', default: false },
      fields: { type: 'boolean', default: false },
      head: { type: 'string', default: '0' },
      tail: { type: 'string', default: '0' },
      follow: { type: 'boolean', default: false },
      sync: { type: 'boolean', default: false },
      flush: { type: 'boolean', default: false },
      rotate: { type: 'boolean', default: false },
      'relinquish-var': { type: 'boolean', default: false },
      verify: { type: 'boolean', default: false },
      'verify-only': { type: 'boolean', default: false },
      'verify-key': { type: 'string' },
      'boot': { type: 'string' },
      'since': { type: 'string' },
      'until': { type: 'string' },
      'no-tail': { type: 'boolean', default: false },
    },
    allowPositionals: true,
  });
} catch (err) {
  process.stderr.write(`Error: ${err.message}\n`);
  process.exit(1);
}

const { values, positionals } = parsed;

function unsupported(name) {
  process.stderr.write(`Error: --${name} is not supported in the pure-JavaScript journalctl\n`);
  process.exit(1);
}

function parseLimit(name, value) {
  if (!/^[0-9]+$/.test(value)) {
    process.stderr.write(`Error: --${name} must be a non-negative integer\n`);
    process.exit(1);
  }
  return Number.parseInt(value, 10);
}

if (values.follow) unsupported('follow');
if (values.sync) unsupported('sync');
if (values.flush) unsupported('flush');
if (values.rotate) unsupported('rotate');
if (values['relinquish-var']) unsupported('relinquish-var');
if (values.boot) unsupported('boot');
if (values.since) unsupported('since');
if (values.until) unsupported('until');

const inputPath = values.file || values.directory;
if (!inputPath) {
  process.stderr.write('Error: use --file or --directory\n');
  process.exit(1);
}

const hasVerifyKey = values['verify-key'] !== undefined;
if (values.verify || values['verify-only'] || hasVerifyKey) {
  process.exit(runVerify(inputPath, values['verify-key'], hasVerifyKey));
}

try {
  const headLimit = parseLimit('head', values.head);
  const tailLimit = parseLimit('tail', values.tail);
  const journal = SdJournalOpen(inputPath, 0);

  // Process positionals as match arguments
  for (const arg of positionals) {
    if (arg === '+') {
      SdJournalAddDisjunction(journal);
    } else if (arg.includes('=')) {
      SdJournalAddMatch(journal, Buffer.from(arg, 'utf8'));
    }
  }

  // Set output mode
  const outputMode = values.output === 'json' ? OUTPUT_MODE_JSON :
                    values.output === 'export' ? OUTPUT_MODE_EXPORT :
                    OUTPUT_MODE_DEFAULT;
  journal.setOutputMode(outputMode);

  if (values['list-boots']) {
    const boots = SdJournalListBoots(journal);
    for (const boot of boots) {
      const first = new Date(boot.first_entry / 1000).toISOString();
      const last = new Date(boot.last_entry / 1000).toISOString();
      const idx = boot.index.toString().padStart(4, ' ');
      process.stdout.write(`[${idx}] ${boot.boot_id.slice(0, 8)} ${first} - ${last}\n`);
    }
    journal.close();
    process.exit(0);
  }

  if (values.fields) {
    const fields = SdJournalEnumerateFields(journal);
    fields.sort();
    for (const field of fields) {
      process.stdout.write(field + '\n');
    }
    journal.close();
    process.exit(0);
  }

  if (tailLimit > 0) {
    SdJournalSeekTail(journal);
    const entries = [];

    for (let i = 0; i < tailLimit; i++) {
      const rc = SdJournalPrevious(journal);
      if (rc === 0) break;

      const entry = SdJournalGetEntry(journal);
      if (entry) {
        const output = SdJournalProcessOutput(journal, entry);
        entries.push(output);
      }
    }

    // Print in reverse (oldest first)
    for (let i = entries.length - 1; i >= 0; i--) {
      const out = entries[i];
      writeSync(1, Buffer.isBuffer(out) ? out : Buffer.from(out));
    }
    journal.close();
    process.exit(0);
  }

  // Default: head or all
  SdJournalSeekHead(journal);
  let count = 0;

  while (true) {
    if (headLimit > 0 && count >= headLimit) break;

    const rc = SdJournalNext(journal);
    if (rc === 0) break;

    const entry = SdJournalGetEntry(journal);
    if (entry) {
      const output = SdJournalProcessOutput(journal, entry);
      writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
    }
    count++;
  }

  journal.close();
  process.exit(0);
} catch (err) {
  process.stderr.write('Error: ' + err.message + '\n');
  process.exit(1);
}

function runVerify(inputPath, verifyKey, hasVerifyKey) {
  if (hasVerifyKey && !validVerificationKey(verifyKey)) {
    process.stderr.write('Failed to parse seed.\n');
    return 1;
  }

  let stats;
  try {
    stats = statSync(inputPath);
  } catch (err) {
    process.stderr.write(`Error: verify: ${err.message}\n`);
    return 1;
  }

  let files = [];
  if (stats.isDirectory()) {
    for (const entry of readdirSync(inputPath)) {
      const candidate = join(inputPath, entry);
      let entryStats;
      try {
        entryStats = statSync(candidate);
      } catch {
        continue;
      }
      if (entryStats.isFile() && isJournalFileName(entry)) {
        files.push(candidate);
      }
    }
    files.sort();
  } else {
    files.push(inputPath);
  }

  if (files.length === 0) {
    process.stderr.write('Error: verify: no journal files found\n');
    return 1;
  }

  let firstErr = null;
  for (const file of files) {
    let sealed = false;
    let r = null;
    try {
      r = FileReader.open(file);
      sealed = (r.header.compatible_flags & COMPATIBLE_SEALED) !== 0;
    } catch (err) {
      process.stderr.write(`FAIL: ${file} (${err.message})\n`);
      if (!firstErr) firstErr = err;
      continue;
    } finally {
      if (r) r.close();
    }

    if (sealed && !hasVerifyKey) {
      process.stderr.write(`Journal file ${file} has sealing enabled but verification key has not been passed using --verify-key=.\n`);
      process.stderr.write(`FAIL: ${file} (verification key required for sealed journal file)\n`);
      if (!firstErr) firstErr = new Error('verification key required for sealed journal file');
      continue;
    }

    if (sealed && hasVerifyKey) {
      try {
        verifyFileWithKey(file, values['verify-key']);
        process.stderr.write(`PASS: ${file}\n`);
      } catch (err) {
        process.stderr.write(`FAIL: ${file} (${err.message})\n`);
        if (!firstErr) firstErr = err;
      }
      continue;
    }

    try {
      verifyFile(file);
      process.stderr.write(`PASS: ${file}\n`);
    } catch (err) {
      process.stderr.write(`FAIL: ${file} (${err.message})\n`);
      if (!firstErr) firstErr = err;
    }
  }

  if (firstErr) {
    process.stderr.write('Error: ' + firstErr.message + '\n');
    return 1;
  }
  return 0;
}

function validVerificationKey(key) {
  let i = 0;
  for (let c = 0; c < 12; c++) {
    while (i < key.length && key[i] === '-') i++;
    if (i + 2 > key.length || !isHex(key[i]) || !isHex(key[i + 1])) {
      return false;
    }
    i += 2;
  }
  if (i >= key.length || key[i] !== '/') {
    return false;
  }
  i++;

  const start = consumeHex(key, i);
  if (!start.ok || start.next >= key.length || key[start.next] !== '-') {
    return false;
  }
  const interval = consumeHex(key, start.next + 1);
  if (!interval.ok || interval.next !== key.length) return false;
  return key.slice(start.next + 1, interval.next).split('').some((ch) => ch !== '0');
}

function consumeHex(s, start) {
  let i = start;
  while (i < s.length && isHex(s[i])) i++;
  return { next: i, ok: i > start };
}

function isHex(ch) {
  return typeof ch === 'string' && /^[0-9a-fA-F]$/.test(ch);
}
