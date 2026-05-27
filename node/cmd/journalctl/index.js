#!/usr/bin/env node
// Pure-JavaScript journalctl for file-backed/query behavior.

import { writeSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import {
  SdJournalOpen, SdJournalAddMatch, SdJournalAddDisjunction,
  SdJournalAddConjunction,
  SdJournalListBoots, SdJournalEnumerateFields, SdJournalSeekHead,
  SdJournalNext, SdJournalGetEntry, SdJournalProcessOutput,
  SdJournalSeekRealtimeUsec,
  OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
} from '../../src/facade.js';
import { verifyFile, verifyFileWithKey } from '../../src/lib/verify.js';
import { FileReader } from '../../src/lib/reader.js';
import { isJournalFileName } from '../../src/lib/compress.js';
import { COMPATIBLE_SEALED } from '../../src/lib/header.js';

let parsed;
let rawArgs;
try {
  rawArgs = preprocessOptionalBootArg(process.argv.slice(2));
  parsed = parseArgs({
    args: rawArgs,
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
      'boot': { type: 'string', short: 'b' },
      'since': { type: 'string', short: 'S' },
      'until': { type: 'string', short: 'U' },
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

function hasOption(args, name) {
  return args.some(arg => arg === `--${name}` || arg.startsWith(`--${name}=`));
}

if (values.sync) unsupported('sync');
if (values.flush) unsupported('flush');
if (values.rotate) unsupported('rotate');
if (values['relinquish-var']) unsupported('relinquish-var');

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
  const sinceUsec = values.since ? parseTimestampUsec(values.since) : null;
  const untilUsec = values.until ? parseTimestampUsec(values.until) : null;
  if (sinceUsec !== null && untilUsec !== null && sinceUsec > untilUsec) {
    throw new Error('--since= must be before --until=.');
  }

  if (values.follow) {
    const followTail = hasOption(rawArgs, 'tail') ? tailLimit : 10;
    await runFollow(inputPath, values, positionals, sinceUsec, untilUsec, followTail);
    process.exit(0);
  }

  const journal = openFilteredJournal(inputPath, values, positionals);

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
    showTail(journal, tailLimit, sinceUsec, untilUsec);
    journal.close();
    process.exit(0);
  }

  showForward(journal, headLimit, sinceUsec, untilUsec);
  journal.close();
  process.exit(0);
} catch (err) {
  process.stderr.write('Error: ' + err.message + '\n');
  process.exit(1);
}

function preprocessOptionalBootArg(args) {
  const out = [];
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--boot' || arg === '-b') {
      const next = args[i + 1];
      if (next !== undefined && looksLikeBootDescriptor(next)) {
        out.push(`${arg}=${next}`);
        i++;
      } else {
        out.push(`${arg}=`);
      }
      continue;
    }
    out.push(arg);
  }
  return out;
}

function looksLikeBootDescriptor(value) {
  return value === 'all' ||
    /^[+-]?\d+$/.test(value) ||
    /^[0-9a-fA-F]{32}([+-]\d+)?$/.test(value) ||
    /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}([+-]\d+)?$/.test(value);
}

function parseTimestampUsec(value) {
  const text = String(value).trim();
  if (text === 'now') return BigInt(Date.now()) * 1000n;
  if (['today', 'yesterday', 'tomorrow'].includes(text)) {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    if (text === 'yesterday') d.setDate(d.getDate() - 1);
    if (text === 'tomorrow') d.setDate(d.getDate() + 1);
    return BigInt(d.getTime()) * 1000n;
  }
  if (text.startsWith('@')) return parseEpochTimestampUsec(text.slice(1));
  if (/^[+-]/.test(text) && !/^[+-]\d{4}-/.test(text)) {
    const delta = parseDurationUsec(text.slice(1));
    const now = BigInt(Date.now()) * 1000n;
    return text[0] === '+' ? now + delta : now - delta;
  }
  const dt = parseDateTimestamp(text);
  if (dt !== null) return dt;
  throw new Error(`failed to parse timestamp: ${value}`);
}

function parseEpochTimestampUsec(value) {
  if (!/^\d+(\.\d+)?$/.test(value)) throw new Error(`failed to parse timestamp: @${value}`);
  const [whole, frac = ''] = value.split('.');
  return BigInt(whole) * 1_000_000n + BigInt((frac + '000000').slice(0, 6));
}

function parseDateTimestamp(value) {
  let m = value.match(/^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,6}))?)?)?$/);
  if (m) {
    const [, y, mo, d, h = '0', mi = '0', s = '0', us = '0'] = m;
    return localDateUsec(y, mo, d, h, mi, s, us);
  }
  m = value.match(/^(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,6}))?)?$/);
  if (m) {
    const now = new Date();
    const [, h, mi, s = '0', us = '0'] = m;
    return localDateUsec(
      String(now.getFullYear()),
      String(now.getMonth() + 1).padStart(2, '0'),
      String(now.getDate()).padStart(2, '0'),
      h, mi, s, us,
    );
  }
  return null;
}

function localDateUsec(y, mo, d, h, mi, s, us) {
  const year = Number(y);
  const month = Number(mo);
  const day = Number(d);
  const hour = Number(h);
  const minute = Number(mi);
  const second = Number(s);
  const usec = Number((us + '000000').slice(0, 6));
  const date = new Date(year, month - 1, day, hour, minute, second, Math.floor(usec / 1000));
  if (date.getFullYear() !== year ||
      date.getMonth() !== month - 1 ||
      date.getDate() !== day ||
      date.getHours() !== hour ||
      date.getMinutes() !== minute ||
      date.getSeconds() !== second) {
    return null;
  }
  return BigInt(date.getTime()) * 1000n + BigInt(usec % 1000);
}

function parseDurationUsec(value) {
  const units = new Map([
    ['us', 1n], ['usec', 1n], ['usecs', 1n],
    ['ms', 1000n], ['msec', 1000n], ['msecs', 1000n],
    ['s', 1_000_000n], ['sec', 1_000_000n], ['secs', 1_000_000n], ['second', 1_000_000n], ['seconds', 1_000_000n],
    ['m', 60_000_000n], ['min', 60_000_000n], ['mins', 60_000_000n], ['minute', 60_000_000n], ['minutes', 60_000_000n],
    ['h', 3_600_000_000n], ['hr', 3_600_000_000n], ['hour', 3_600_000_000n], ['hours', 3_600_000_000n],
    ['d', 86_400_000_000n], ['day', 86_400_000_000n], ['days', 86_400_000_000n],
    ['w', 604_800_000_000n], ['week', 604_800_000_000n], ['weeks', 604_800_000_000n],
  ]);
  const re = /\s*(\d+(?:\.\d+)?)(?:\s*([A-Za-z]+))?/gy;
  let total = 0n;
  let pos = 0;
  let match;
  while ((match = re.exec(value)) !== null) {
    pos = re.lastIndex;
    const unit = (match[2] || 's').toLowerCase();
    const multiplier = units.get(unit);
    if (!multiplier) throw new Error(`failed to parse duration: ${value}`);
    const [whole, frac = ''] = match[1].split('.');
    const scale = 10n ** BigInt(frac.length);
    total += (BigInt(whole) * scale + BigInt(frac || '0')) * multiplier / scale;
  }
  if (pos !== value.length || total === 0n) throw new Error(`failed to parse duration: ${value}`);
  return total;
}

function openFilteredJournal(inputPath, opts, matches) {
  const journal = SdJournalOpen(inputPath, 0);
  try {
    if (opts.boot !== undefined && String(opts.boot).trim() !== 'all') {
      const bootId = resolveBootId(journal, String(opts.boot));
      if (bootId) {
        SdJournalAddMatch(journal, Buffer.from(`_BOOT_ID=${bootId}`, 'ascii'));
        SdJournalAddConjunction(journal);
      }
    }
    for (const arg of matches) {
      if (arg === '+') SdJournalAddDisjunction(journal);
      else if (arg.includes('=')) SdJournalAddMatch(journal, Buffer.from(arg, 'utf8'));
    }
    const outputMode = opts.output === 'json' ? OUTPUT_MODE_JSON :
                      opts.output === 'export' ? OUTPUT_MODE_EXPORT :
                      OUTPUT_MODE_DEFAULT;
    journal.setOutputMode(outputMode);
    return journal;
  } catch (err) {
    journal.close();
    throw err;
  }
}

function collectBoots(journal) {
  const boots = new Map();
  SdJournalSeekHead(journal);
  for (;;) {
    const rc = SdJournalNext(journal);
    if (rc === 0) break;
    const entry = SdJournalGetEntry(journal);
    if (!entry || !entry.boot_id) continue;
    const bootId = Buffer.from(entry.boot_id).toString('hex');
    if (!bootId || /^0+$/.test(bootId)) continue;
    const realtime = BigInt(entry.realtime || 0n);
    const item = boots.get(bootId);
    if (item) {
      if (realtime < item.first_entry) item.first_entry = realtime;
      if (realtime > item.last_entry) item.last_entry = realtime;
    } else {
      boots.set(bootId, { boot_id: bootId, first_entry: realtime, last_entry: realtime });
    }
  }
  const result = Array.from(boots.values()).sort((a, b) => {
    if (a.first_entry !== b.first_entry) return a.first_entry < b.first_entry ? -1 : 1;
    return a.boot_id.localeCompare(b.boot_id);
  });
  const base = 1 - result.length;
  for (let i = 0; i < result.length; i++) result[i].index = base + i;
  return result;
}

function resolveBootId(journal, descriptor) {
  if (descriptor === 'all') return null;
  const { bootId, offset } = parseBootDescriptor(descriptor);
  const boots = collectBoots(journal);
  if (boots.length === 0) throw new Error('no journal boot entry found for the specified boot');
  let target;
  if (bootId) {
    const base = boots.findIndex(b => b.boot_id === bootId);
    if (base < 0) throw new Error(`no journal boot entry found for the specified boot (${bootId}${formatOffset(offset)})`);
    target = base + offset;
  } else if (offset > 0) {
    target = offset - 1;
  } else {
    target = boots.length - 1 + offset;
  }
  if (target < 0 || target >= boots.length) {
    throw new Error(`no journal boot entry found for the specified boot (${bootId || ''}${formatOffset(offset)})`);
  }
  return boots[target].boot_id;
}

function parseBootDescriptor(descriptor) {
  if (descriptor === '') return { bootId: '', offset: 0 };
  const m = descriptor.match(/^(([0-9A-Fa-f]{32})|([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}))?([+-]?\d+)?$/);
  if (!m) throw new Error(`failed to parse boot descriptor: ${descriptor}`);
  return {
    bootId: (m[1] || '').replace(/-/g, '').toLowerCase(),
    offset: m[4] === undefined || m[4] === '' ? 0 : Number.parseInt(m[4], 10),
  };
}

function formatOffset(offset) {
  return offset >= 0 ? `+${offset}` : String(offset);
}

function entryInTimeRange(entry, sinceUsec, untilUsec) {
  const realtime = BigInt(entry.realtime || 0n);
  if (sinceUsec !== null && realtime < sinceUsec) return false;
  if (untilUsec !== null && realtime > untilUsec) return false;
  return true;
}

function* iterMatchingEntries(journal, sinceUsec, untilUsec) {
  if (sinceUsec !== null) SdJournalSeekRealtimeUsec(journal, sinceUsec);
  else SdJournalSeekHead(journal);
  for (;;) {
    const rc = SdJournalNext(journal);
    if (rc === 0) break;
    const entry = SdJournalGetEntry(journal);
    if (!entry) continue;
    const realtime = BigInt(entry.realtime || 0n);
    if (untilUsec !== null && realtime > untilUsec) break;
    if (entryInTimeRange(entry, sinceUsec, untilUsec)) yield entry;
  }
}

function showForward(journal, headLimit, sinceUsec, untilUsec) {
  let count = 0;
  for (const entry of iterMatchingEntries(journal, sinceUsec, untilUsec)) {
    if (headLimit > 0 && count >= headLimit) break;
    const output = SdJournalProcessOutput(journal, entry);
    writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
    count++;
  }
}

function showTail(journal, tailLimit, sinceUsec, untilUsec) {
  const outputs = [];
  for (const entry of iterMatchingEntries(journal, sinceUsec, untilUsec)) {
    outputs.push(SdJournalProcessOutput(journal, entry));
  }
  for (const output of outputs.slice(-tailLimit)) {
    writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
  }
}

function scanFollowSnapshot(inputPath, opts, matches, sinceUsec, untilUsec) {
  let journal;
  try {
    journal = openFilteredJournal(inputPath, opts, matches);
  } catch {
    return [];
  }
  try {
    const out = [];
    for (const entry of iterMatchingEntries(journal, sinceUsec, untilUsec)) {
      if (!entry.cursor) continue;
      out.push([entry.cursor, SdJournalProcessOutput(journal, entry)]);
    }
    return out;
  } finally {
    journal.close();
  }
}

async function runFollow(inputPath, opts, matches, sinceUsec, untilUsec, tailLimit) {
  const seen = new Set();
  const initial = scanFollowSnapshot(inputPath, opts, matches, sinceUsec, untilUsec);
  for (const [cursor] of initial) seen.add(cursor);
  const toPrint = opts['no-tail'] || sinceUsec !== null ? initial : initial.slice(-tailLimit);
  for (const [, output] of toPrint) writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
  for (;;) {
    await new Promise(resolve => setTimeout(resolve, 100));
    const snapshot = scanFollowSnapshot(inputPath, opts, matches, sinceUsec, untilUsec);
    for (const [cursor, output] of snapshot) {
      if (seen.has(cursor)) continue;
      seen.add(cursor);
      writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
    }
  }
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
  const directoryInput = stats.isDirectory();
  if (directoryInput) {
    files = collectJournalFilesForVerify(inputPath);
  } else {
    files.push(inputPath);
  }

  if (files.length === 0) {
    if (directoryInput) return 0;
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
      if (directoryInput) continue;
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
        verifyFileWithKey(file, verifyKey);
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

function collectJournalFilesForVerify(path) {
  const files = [];
  const entries = readdirSync(path, { withFileTypes: true });
  for (const entry of entries) {
    const candidate = join(path, entry.name);
    if (isRegularFile(candidate) && isJournalFileName(entry.name)) files.push(candidate);
  }
  for (const entry of entries) {
    if (!isJournalSubdirName(entry.name)) continue;
    const childPath = join(path, entry.name);
    if (!isDirectory(childPath)) continue;
    for (const child of readDirEntries(childPath)) {
      const candidate = join(childPath, child.name);
      if (isRegularFile(candidate) && isJournalFileName(child.name)) files.push(candidate);
    }
  }
  return files.sort();
}

function isRegularFile(path) {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function readDirEntries(path) {
  try {
    return readdirSync(path, { withFileTypes: true });
  } catch {
    return [];
  }
}

function isJournalSubdirName(name) {
  if (name.includes('.')) return false;
  return id128StringValid(name);
}

function id128StringValid(s) {
  if (s.length === 32) return /^[0-9a-fA-F]{32}$/.test(s);
  if (s.length === 36) return /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(s);
  return false;
}
