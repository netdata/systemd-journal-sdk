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
const FOLLOW_SLEEP_STATE = new Int32Array(new SharedArrayBuffer(4));
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
  if (!isUnsignedDecimal(value)) {
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
    runFollow(inputPath, values, positionals, sinceUsec, untilUsec, followTail);
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
    const arg = args.at(i);
    if (arg === '--boot' || arg === '-b') {
      const next = args.at(i + 1);
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
  if (value === 'all') return true;
  if (value === '') return false;
  try {
    parseBootDescriptor(value);
    return true;
  } catch {
    return false;
  }
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
  if (startsWithSign(text) && !startsWithSignedDate(text)) {
    const delta = parseDurationUsec(text.slice(1));
    const now = BigInt(Date.now()) * 1000n;
    return text.charAt(0) === '+' ? now + delta : now - delta;
  }
  const dt = parseDateTimestamp(text);
  if (dt !== null) return dt;
  throw new Error(`failed to parse timestamp: ${value}`);
}

function parseEpochTimestampUsec(value) {
  if (!isDecimalNumber(value)) throw new Error(`failed to parse timestamp: @${value}`);
  const [whole, frac = ''] = value.split('.');
  return BigInt(whole) * 1_000_000n + BigInt((frac + '000000').slice(0, 6));
}

function parseDateTimestamp(value) {
  const split = splitDateTime(value);
  if (split !== null) {
    const time = split.timePart === '' ? ['0', '0', '0', '0'] : parseTimeParts(split.timePart);
    if (time === null) return null;
    return localDateUsec(...split.dateParts, ...time);
  }
  const time = parseTimeParts(value);
  if (time !== null) {
    const now = new Date();
    return localDateUsec(
      String(now.getFullYear()),
      String(now.getMonth() + 1).padStart(2, '0'),
      String(now.getDate()).padStart(2, '0'),
      ...time,
    );
  }
  return null;
}

function splitDateTime(value) {
  if (value.length < 10 || value[4] !== '-' || value[7] !== '-') return null;
  const dateText = value.slice(0, 10);
  const dateParts = parseDateParts(dateText);
  if (dateParts === null) return null;
  if (value.length === 10) return { dateParts, timePart: '' };
  const separator = value[10];
  if (separator !== ' ' && separator !== 'T') return null;
  return { dateParts, timePart: value.slice(11) };
}

function parseDateParts(text) {
  if (text.length !== 10 || text.charAt(4) !== '-' || text.charAt(7) !== '-') return null;
  const y = text.slice(0, 4);
  const mo = text.slice(5, 7);
  const d = text.slice(8, 10);
  if (!isNDigits(y, 4) || !isNDigits(mo, 2) || !isNDigits(d, 2)) return null;
  return [y, mo, d];
}

function parseTimeParts(text) {
  const dot = text.indexOf('.');
  const main = dot < 0 ? text : text.slice(0, dot);
  const us = dot < 0 ? '0' : text.slice(dot + 1);
  if (dot >= 0 && !isFractionUsec(us)) return null;
  const parts = main.split(':');
  if (parts.length !== 2 && parts.length !== 3) return null;
  const [h, mi, s = '0'] = parts;
  if (!isNDigits(h, 2) || !isNDigits(mi, 2)) return null;
  if (parts.length === 3 && !isNDigits(s, 2)) return null;
  return [h, mi, s, us];
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
  let total = 0n;
  let pos = 0;
  while (pos < value.length) {
    const part = parseDurationPart(value, pos);
    if (part === null) break;
    total += part.usec;
    pos = part.next;
  }
  if (pos !== value.length || total === 0n) throw new Error(`failed to parse duration: ${value}`);
  return total;
}

const DURATION_UNITS = new Map([
  ['us', 1n], ['usec', 1n], ['usecs', 1n],
  ['ms', 1000n], ['msec', 1000n], ['msecs', 1000n],
  ['s', 1_000_000n], ['sec', 1_000_000n], ['secs', 1_000_000n], ['second', 1_000_000n], ['seconds', 1_000_000n],
  ['m', 60_000_000n], ['min', 60_000_000n], ['mins', 60_000_000n], ['minute', 60_000_000n], ['minutes', 60_000_000n],
  ['h', 3_600_000_000n], ['hr', 3_600_000_000n], ['hour', 3_600_000_000n], ['hours', 3_600_000_000n],
  ['d', 86_400_000_000n], ['day', 86_400_000_000n], ['days', 86_400_000_000n],
  ['w', 604_800_000_000n], ['week', 604_800_000_000n], ['weeks', 604_800_000_000n],
]);

function parseDurationPart(value, start) {
  const ws = skipDurationWhitespace(value, start);
  if (ws.pos >= value.length) {
    if (ws.skipped) throw new Error(`failed to parse duration: ${value}`);
    return null;
  }
  const number = readDurationNumber(value, ws.pos);
  const unit = readDurationUnit(value, number.next);
  const multiplier = DURATION_UNITS.get(unit.name);
  if (!multiplier) throw new Error(`failed to parse duration: ${value}`);
  return { next: unit.next, usec: durationNumberUsec(number.text, multiplier) };
}

function skipDurationWhitespace(value, pos) {
  const start = pos;
  while (pos < value.length && isWhitespace(value.charAt(pos))) pos++;
  return { pos, skipped: pos !== start };
}

function readDurationNumber(value, pos) {
  const start = pos;
  while (pos < value.length && isDigit(value.charAt(pos))) pos++;
  if (pos < value.length && value.charAt(pos) === '.') {
    pos = readDurationFraction(value, pos + 1);
  }
  if (pos === start) throw new Error(`failed to parse duration: ${value}`);
  return { text: value.slice(start, pos), next: pos };
}

function readDurationFraction(value, pos) {
  const start = pos;
  while (pos < value.length && isDigit(value.charAt(pos))) pos++;
  if (pos === start) throw new Error(`failed to parse duration: ${value}`);
  return pos;
}

function readDurationUnit(value, pos) {
  while (pos < value.length && isWhitespace(value.charAt(pos))) pos++;
  const start = pos;
  while (pos < value.length && isAsciiLetter(value.charAt(pos))) pos++;
  return { name: (pos === start ? 's' : value.slice(start, pos)).toLowerCase(), next: pos };
}

function durationNumberUsec(numberText, multiplier) {
  const [whole, frac = ''] = numberText.split('.');
  const scale = 10n ** BigInt(frac.length);
  return (BigInt(whole) * scale + BigInt(frac || '0')) * multiplier / scale;
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
    if (!bootId || isAllZeros(bootId)) continue;
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
  for (const [i, boot] of result.entries()) boot.index = base + i;
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
  return boots.at(target).boot_id;
}

function parseBootDescriptor(descriptor) {
  if (descriptor === '') return { bootId: '', offset: 0 };
  if (isSignedDecimal(descriptor)) return { bootId: '', offset: Number.parseInt(descriptor, 10) };

  const directBootId = normalizeBootIdText(descriptor);
  if (directBootId !== null) return { bootId: directBootId, offset: 0 };

  for (const idLength of [32, 36]) {
    if (descriptor.length <= idLength) continue;
    const sign = descriptor.charAt(idLength);
    if (sign !== '+' && sign !== '-') continue;
    const bootId = normalizeBootIdText(descriptor.slice(0, idLength));
    const offsetText = descriptor.slice(idLength);
    if (bootId !== null && isSignedDecimal(offsetText)) {
      return { bootId, offset: Number.parseInt(offsetText, 10) };
    }
  }

  throw new Error(`failed to parse boot descriptor: ${descriptor}`);
}

function normalizeBootIdText(text) {
  if (text.length === 32 && isHexString(text)) return text.toLowerCase();
  if (text.length === 36 && isUuidString(text)) return text.replaceAll('-', '').toLowerCase();
  return null;
}

function isUnsignedDecimal(text) {
  if (typeof text !== 'string' || text.length === 0) return false;
  for (const ch of text) {
    if (!isDigit(ch)) return false;
  }
  return true;
}

function isSignedDecimal(text) {
  if (typeof text !== 'string' || text.length === 0) return false;
  const start = startsWithSign(text) ? 1 : 0;
  return start < text.length && isUnsignedDecimal(text.slice(start));
}

function isDecimalNumber(text) {
  if (typeof text !== 'string' || text.length === 0) return false;
  const dot = text.indexOf('.');
  if (dot < 0) return isUnsignedDecimal(text);
  if (text.indexOf('.', dot + 1) >= 0) return false;
  return dot > 0 && dot + 1 < text.length &&
    isUnsignedDecimal(text.slice(0, dot)) &&
    isUnsignedDecimal(text.slice(dot + 1));
}

function startsWithSign(text) {
  return text.length > 0 && (text.charAt(0) === '+' || text.charAt(0) === '-');
}

function startsWithSignedDate(text) {
  return text.length >= 6 && startsWithSign(text) &&
    isNDigits(text.slice(1, 5), 4) && text.charAt(5) === '-';
}

function isNDigits(text, count) {
  return text.length === count && isUnsignedDecimal(text);
}

function isFractionUsec(text) {
  return text.length >= 1 && text.length <= 6 && isUnsignedDecimal(text);
}

function isDigit(ch) {
  return ch >= '0' && ch <= '9';
}

function isAsciiLetter(ch) {
  return (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z');
}

function isWhitespace(ch) {
  return ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r' || ch === '\v' || ch === '\f';
}

function isAllZeros(text) {
  for (const ch of text) {
    if (ch !== '0') return false;
  }
  return text.length > 0;
}

function isHexString(text) {
  for (const ch of text) {
    if (!isHex(ch)) return false;
  }
  return text.length > 0;
}

function isUuidString(text) {
  if (text.length !== 36) return false;
  const hyphens = new Set([8, 13, 18, 23]);
  for (let i = 0; i < text.length; i++) {
    if (hyphens.has(i)) {
      if (text.charAt(i) !== '-') return false;
    } else if (!isHex(text.charAt(i))) {
      return false;
    }
  }
  return true;
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

function sleepMs(milliseconds) {
  Atomics.wait(FOLLOW_SLEEP_STATE, 0, 0, milliseconds);
}

function runFollow(inputPath, opts, matches, sinceUsec, untilUsec, tailLimit) {
  const seen = new Set();
  const initial = scanFollowSnapshot(inputPath, opts, matches, sinceUsec, untilUsec);
  for (const [cursor] of initial) seen.add(cursor);
  const toPrint = opts['no-tail'] || sinceUsec !== null ? initial : initial.slice(-tailLimit);
  for (const [, output] of toPrint) writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
  for (;;) {
    try {
      sleepMs(100);
      const snapshot = scanFollowSnapshot(inputPath, opts, matches, sinceUsec, untilUsec);
      for (const [cursor, output] of snapshot) {
        if (seen.has(cursor)) continue;
        seen.add(cursor);
        writeSync(1, Buffer.isBuffer(output) ? output : Buffer.from(output));
      }
    } catch (err) {
      process.stderr.write(`Error: follow: ${err.message}\n`);
      process.exit(1);
    }
  }
}

function runVerify(inputPath, verifyKey, hasVerifyKey) {
  if (hasVerifyKey && !validVerificationKey(verifyKey)) {
    process.stderr.write('Failed to parse seed.\n');
    return 1;
  }

  const input = verificationInputFiles(inputPath);
  if (!input) return 1;

  if (input.files.length === 0) {
    if (input.directoryInput) return 0;
    process.stderr.write('Error: verify: no journal files found\n');
    return 1;
  }

  const firstErr = verifyJournalFiles(input.files, input.directoryInput, verifyKey, hasVerifyKey);
  if (firstErr) {
    process.stderr.write('Error: ' + firstErr.message + '\n');
    return 1;
  }
  return 0;
}

function verificationInputFiles(inputPath) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- journalctl verifies the explicit --file/--directory target.
    const stats = statSync(inputPath);
    const directoryInput = stats.isDirectory();
    return {
      directoryInput,
      files: directoryInput ? collectJournalFilesForVerify(inputPath) : [inputPath],
    };
  } catch (err) {
    process.stderr.write(`Error: verify: ${err.message}\n`);
    return null;
  }
}

function verifyJournalFiles(files, directoryInput, verifyKey, hasVerifyKey) {
  let firstErr = null;
  for (const file of files) {
    const err = verifyJournalFile(file, directoryInput, verifyKey, hasVerifyKey);
    if (err && !firstErr) firstErr = err;
  }
  return firstErr;
}

function verifyJournalFile(file, directoryInput, verifyKey, hasVerifyKey) {
  const sealed = readJournalSealedFlag(file, directoryInput);
  if (sealed === null) return null;
  if (sealed instanceof Error) return sealed;
  if (sealed && !hasVerifyKey) return verificationKeyRequiredError(file);
  return runJournalFileVerification(file, sealed, verifyKey);
}

function readJournalSealedFlag(file, directoryInput) {
  let r = null;
  try {
    r = FileReader.open(file, {});
    return (r.header.compatible_flags & COMPATIBLE_SEALED) !== 0;
  } catch (err) {
    if (directoryInput) return null;
    process.stderr.write(`FAIL: ${file} (${err.message})\n`);
    return err;
  } finally {
    if (r) r.close();
  }
}

function verificationKeyRequiredError(file) {
  const err = new Error('verification key required for sealed journal file');
  process.stderr.write(`Journal file ${file} has sealing enabled but verification key has not been passed using --verify-key=.\n`);
  process.stderr.write(`FAIL: ${file} (${err.message})\n`);
  return err;
}

function runJournalFileVerification(file, sealed, verifyKey) {
  try {
    if (sealed) verifyFileWithKey(file, verifyKey);
    else verifyFile(file);
    process.stderr.write(`PASS: ${file}\n`);
    return null;
  } catch (err) {
    process.stderr.write(`FAIL: ${file} (${err.message})\n`);
    return err;
  }
}

function validVerificationKey(key) {
  const seedEnd = consumeVerificationKeySeed(key);
  if (seedEnd === null || seedEnd >= key.length || key.charAt(seedEnd) !== '/') return false;
  const start = consumeHex(key, seedEnd + 1);
  if (!start.ok || start.next >= key.length || key.charAt(start.next) !== '-') {
    return false;
  }
  const interval = consumeHex(key, start.next + 1);
  if (!interval.ok || interval.next !== key.length) return false;
  return key.slice(start.next + 1, interval.next).split('').some((ch) => ch !== '0');
}

function consumeVerificationKeySeed(key) {
  let i = 0;
  for (let c = 0; c < 12; c++) {
    while (i < key.length && key.charAt(i) === '-') i++;
    if (!hasHexPairAt(key, i)) return null;
    i += 2;
  }
  return i;
}

function hasHexPairAt(key, offset) {
  return offset + 2 <= key.length && isHex(key.charAt(offset)) && isHex(key.charAt(offset + 1));
}

function consumeHex(s, start) {
  let i = start;
  while (i < s.length && isHex(s.charAt(i))) i++;
  return { next: i, ok: i > start };
}

function isHex(ch) {
  return typeof ch === 'string' && ch.length === 1 &&
    ((ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f') || (ch >= 'A' && ch <= 'F'));
}

function collectJournalFilesForVerify(path) {
  const files = [];
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- directory target is explicit CLI input.
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
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- helper validates candidate paths found under explicit input.
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isDirectory(path) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- helper validates candidate paths found under explicit input.
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function readDirEntries(path) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- helper lists candidate subdirectories under explicit input.
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
  return normalizeBootIdText(s) !== null;
}
