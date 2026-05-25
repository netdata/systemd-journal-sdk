#!/usr/bin/env python3
# Pure-Python journalctl for file-backed/query behavior.

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from journal import (
    SdJournalOpen, SdJournalAddMatch, SdJournalAddDisjunction,
    SdJournalListBoots, SdJournalEnumerateFields, SdJournalSeekHead,
    SdJournalNext, SdJournalGetEntry, SdJournalProcessOutput,
    SdJournalSeekTail, SdJournalPrevious, SdJournalSetOutputMode,
    OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
)
from journal.reader import FileReader
from journal.verify import verify_file
from journal.compress import is_journal_file_name
from journal.header import COMPATIBLE_SEALED


def unsupported(name):
    sys.stderr.write(f'Error: --{name} is not supported in the pure-Python journalctl\n')
    sys.exit(1)


def parse_limit(name, value):
    try:
        v = int(value, 10)
        if v < 0:
            raise ValueError('negative')
        return v
    except ValueError:
        sys.stderr.write(f'Error: --{name} must be a non-negative integer\n')
        sys.exit(1)


def run_verify(input_path, verify_key):
    has_verify_key = verify_key is not None
    if has_verify_key and not valid_verification_key(verify_key):
        sys.stderr.write('Failed to parse seed.\n')
        return 1

    if os.path.isdir(input_path):
        files = sorted(
            os.path.join(input_path, name)
            for name in os.listdir(input_path)
            if is_journal_file_name(name) and os.path.isfile(os.path.join(input_path, name))
        )
    else:
        files = [input_path]

    if not files:
        sys.stderr.write('Error: verify: no journal files found\n')
        return 1

    first_err = None
    for path in files:
        r = None
        try:
            r = FileReader.open(path)
            sealed = (r.header()['compatible_flags'] & COMPATIBLE_SEALED) != 0
        except Exception as err:
            sys.stderr.write(f'FAIL: {path} ({err})\n')
            if first_err is None:
                first_err = err
            continue
        finally:
            if r is not None:
                r.close()

        if sealed and has_verify_key:
            msg = 'sealed FSS verification is not yet implemented'
            sys.stderr.write(f'FAIL: {path} ({msg})\n')
            if first_err is None:
                first_err = RuntimeError(msg)
            continue

        if sealed and not has_verify_key:
            sys.stderr.write(
                f'Journal file {path} has sealing enabled but verification key '
                f'has not been passed using --verify-key=.\n'
            )
            sys.stderr.write(f'FAIL: {path} (verification key required for sealed journal file)\n')
            if first_err is None:
                first_err = RuntimeError('verification key required for sealed journal file')
            continue

        try:
            verify_file(path)
            sys.stderr.write(f'PASS: {path}\n')
        except Exception as err:
            sys.stderr.write(f'FAIL: {path} ({err})\n')
            if first_err is None:
                first_err = err

    if first_err is not None:
        sys.stderr.write(f'Error: {first_err}\n')
        return 1
    return 0


def valid_verification_key(key):
    i = 0
    for _ in range(12):
        while i < len(key) and key[i] == '-':
            i += 1
        if i + 2 > len(key) or not is_hex(key[i]) or not is_hex(key[i + 1]):
            return False
        i += 2
    if i >= len(key) or key[i] != '/':
        return False
    i += 1

    next_i, ok = consume_hex(key, i)
    if not ok or next_i >= len(key) or key[next_i] != '-':
        return False
    _, ok = consume_hex(key, next_i + 1)
    return ok


def consume_hex(value, start):
    i = start
    while i < len(value) and is_hex(value[i]):
        i += 1
    return i, i > start


def is_hex(ch):
    return ch in '0123456789abcdefABCDEF'


def main():
    parser = argparse.ArgumentParser(description='Pure-Python systemd journal reader')
    parser.add_argument('-f', '--file', help='journal file')
    parser.add_argument('-d', '--directory', help='journal directory')
    parser.add_argument('--output', default='default', choices=['default', 'json', 'export'],
                        help='output mode')
    parser.add_argument('--list-boots', action='store_true')
    parser.add_argument('--fields', action='store_true')
    parser.add_argument('--head', default='0', help='show first N entries')
    parser.add_argument('--tail', default='0', help='show last N entries')
    parser.add_argument('--follow', action='store_true', help='unsupported')
    parser.add_argument('--sync', action='store_true', help='unsupported')
    parser.add_argument('--flush', action='store_true', help='unsupported')
    parser.add_argument('--rotate', action='store_true', help='unsupported')
    parser.add_argument('--relinquish-var', action='store_true', help='unsupported')
    parser.add_argument('--verify', action='store_true', help='verify journal file')
    parser.add_argument('--verify-only', action='store_true', help='verify only')
    parser.add_argument('--verify-key', help='FSS verification key')
    parser.add_argument('--boot', help='unsupported')
    parser.add_argument('--since', help='unsupported')
    parser.add_argument('--until', help='unsupported')
    parser.add_argument('--no-tail', action='store_true')
    parser.add_argument('positional', nargs='*', help='match expressions or +')

    args = parser.parse_args()

    if args.follow:
        unsupported('follow')
    if args.sync:
        unsupported('sync')
    if args.flush:
        unsupported('flush')
    if args.rotate:
        unsupported('rotate')
    if args.relinquish_var:
        unsupported('relinquish-var')
    if args.boot:
        unsupported('boot')
    if args.since:
        unsupported('since')
    if args.until:
        unsupported('until')

    path = args.file or args.directory
    if not path:
        sys.stderr.write('Error: use --file or --directory\n')
        sys.exit(1)

    if args.verify or args.verify_only or args.verify_key is not None:
        return run_verify(path, args.verify_key)

    head_limit = parse_limit('head', args.head)
    tail_limit = parse_limit('tail', args.tail)

    try:
        journal = SdJournalOpen(path, 0)

        for arg in args.positional:
            if arg == '+':
                SdJournalAddDisjunction(journal)
            elif '=' in arg:
                SdJournalAddMatch(journal, arg.encode('latin1'))

        output_mode = OUTPUT_MODE_DEFAULT
        if args.output == 'json':
            output_mode = OUTPUT_MODE_JSON
        elif args.output == 'export':
            output_mode = OUTPUT_MODE_EXPORT
        journal.set_output_mode(output_mode)

        if args.list_boots:
            boots = SdJournalListBoots(journal)
            for boot in boots:
                first = boot['first_entry'] // 1000000
                last = boot['last_entry'] // 1000000
                idx = str(boot['index']).rjust(4)
                from datetime import datetime
                first_dt = datetime.fromtimestamp(first)
                last_dt = datetime.fromtimestamp(last)
                sys.stdout.write(f'[{idx}] {boot["boot_id"][:8]} {first_dt.isoformat()} - {last_dt.isoformat()}\n')
            journal.close()
            return 0

        if args.fields:
            fields = SdJournalEnumerateFields(journal)
            if isinstance(fields, set):
                fields = sorted(fields)
            for f in fields:
                sys.stdout.write(f + '\n')
            journal.close()
            return 0

        if tail_limit > 0:
            SdJournalSeekTail(journal)
            entries = []
            for _ in range(tail_limit):
                rc = SdJournalPrevious(journal)
                if rc == 0:
                    break
                entry = SdJournalGetEntry(journal)
                if entry:
                    out = SdJournalProcessOutput(journal, entry)
                    if isinstance(out, bytes):
                        entries.append(out)
                    else:
                        entries.append(out.encode('utf-8'))
            for out in reversed(entries):
                sys.stdout.buffer.write(out)
            journal.close()
            return 0

        SdJournalSeekHead(journal)
        count = 0
        while True:
            if head_limit > 0 and count >= head_limit:
                break
            rc = SdJournalNext(journal)
            if rc == 0:
                break
            entry = SdJournalGetEntry(journal)
            if entry:
                out = SdJournalProcessOutput(journal, entry)
                if isinstance(out, bytes):
                    sys.stdout.buffer.write(out)
                else:
                    sys.stdout.write(out)
            count += 1

        journal.close()
        return 0
    except Exception as e:
        sys.stderr.write(f'Error: {e}\n')
        sys.exit(1)


if __name__ == '__main__':
    sys.exit(main())
