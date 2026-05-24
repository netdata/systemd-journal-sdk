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
    parser.add_argument('--verify', action='store_true', help='unsupported')
    parser.add_argument('--verify-only', action='store_true', help='unsupported')
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
    if args.verify:
        unsupported('verify')
    if args.verify_only:
        unsupported('verify-only')
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
                        out = out.decode('utf-8', errors='replace')
                    entries.append(out)
            for out in reversed(entries):
                sys.stdout.write(out)
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
                    out = out.decode('utf-8', errors='replace')
                sys.stdout.write(out)
            count += 1

        journal.close()
        return 0
    except Exception as e:
        sys.stderr.write(f'Error: {e}\n')
        sys.exit(1)


if __name__ == '__main__':
    sys.exit(main())