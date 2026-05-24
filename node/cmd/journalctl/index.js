#!/usr/bin/env node
// Pure-JavaScript journalctl for file-backed/query behavior.

import { writeSync } from 'node:fs';
import { parseArgs } from 'node:util';
import {
  SdJournalOpen, SdJournalAddMatch, SdJournalAddDisjunction,
  SdJournalListBoots, SdJournalEnumerateFields, SdJournalSeekHead,
  SdJournalNext, SdJournalGetEntry, SdJournalProcessOutput,
  SdJournalSeekTail, SdJournalPrevious,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from '../../src/facade.js';

const { values, positionals } = parseArgs({
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
    'boot': { type: 'string' },
    'since': { type: 'string' },
    'until': { type: 'string' },
    'no-tail': { type: 'boolean', default: false },
  },
  allowPositionals: true,
});

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
if (values.verify) unsupported('verify');
if (values['verify-only']) unsupported('verify-only');
if (values.boot) unsupported('boot');
if (values.since) unsupported('since');
if (values.until) unsupported('until');

const path = values.file || values.directory;
if (!path) {
  process.stderr.write('Error: use --file or --directory\n');
  process.exit(1);
}

try {
  const headLimit = parseLimit('head', values.head);
  const tailLimit = parseLimit('tail', values.tail);
  const journal = SdJournalOpen(path, 0);

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
