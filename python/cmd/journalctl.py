#!/usr/bin/env python3
# Pure-Python journalctl for file-backed/query behavior.

import argparse
import re
import os
import sys
import time
from datetime import datetime, timedelta

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from journal import (
    SdJournalOpen, SdJournalAddMatch, SdJournalAddDisjunction,
    SdJournalAddConjunction, SdJournalListBoots, SdJournalEnumerateFields, SdJournalSeekHead,
    SdJournalNext, SdJournalGetEntry, SdJournalProcessOutput,
    SdJournalSetOutputMode, SdJournalSeekRealtimeUsec,
    OUTPUT_MODE_DEFAULT, OUTPUT_MODE_JSON, OUTPUT_MODE_EXPORT,
)
from journal.reader import FileReader
from journal.directory_reader import _collect_journal_files
from journal.verify import verify_file, verify_file_with_key, VerificationError
from journal.compress import is_journal_file_name
from journal.header import COMPATIBLE_SEALED


def unsupported(name):
    sys.stderr.write(f'Error: --{name} is not supported in the pure-Python journalctl\n')
    sys.exit(1)


def preprocess_optional_boot_args(argv):
    out = []
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg in ('--boot', '-b'):
            next_arg = argv[i + 1] if i + 1 < len(argv) else None
            if next_arg is not None and looks_like_boot_descriptor(next_arg):
                out.append(f'{arg}={next_arg}')
                i += 2
            else:
                out.append(f'{arg}=')
                i += 1
            continue
        out.append(arg)
        i += 1
    return out


def looks_like_boot_descriptor(value):
    return (
        value == 'all' or
        re.match(r'^[+-]?\d+$', value) is not None or
        re.match(r'^[0-9A-Fa-f]{32}([+-]\d+)?$', value) is not None or
        re.match(r'^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}([+-]\d+)?$', value) is not None
    )


def parse_limit(name, value):
    try:
        v = int(value, 10)
        if v < 0:
            raise ValueError('negative')
        return v
    except ValueError:
        sys.stderr.write(f'Error: --{name} must be a non-negative integer\n')
        sys.exit(1)


def parse_timestamp_usec(value):
    value = value.strip()
    if value == 'now':
        return int(time.time() * 1_000_000)
    if value in ('today', 'yesterday', 'tomorrow'):
        today = datetime.now().replace(hour=0, minute=0, second=0, microsecond=0)
        if value == 'yesterday':
            today -= timedelta(days=1)
        elif value == 'tomorrow':
            today += timedelta(days=1)
        return int(today.timestamp() * 1_000_000)
    if value.startswith('@'):
        return parse_epoch_timestamp_usec(value[1:])
    if value and value[0] in '+-' and len(value) > 1 and not re.match(r'^[+-]\d{4}-', value):
        delta = parse_duration_usec(value[1:])
        now = int(time.time() * 1_000_000)
        return now + delta if value[0] == '+' else now - delta

    now = datetime.now()
    for fmt in (
        '%Y-%m-%d %H:%M:%S.%f',
        '%Y-%m-%d %H:%M:%S',
        '%Y-%m-%d %H:%M',
        '%Y-%m-%d',
        '%H:%M:%S.%f',
        '%H:%M:%S',
        '%H:%M',
    ):
        try:
            dt = datetime.strptime(value, fmt)
        except ValueError:
            continue
        if fmt.startswith('%H'):
            dt = dt.replace(year=now.year, month=now.month, day=now.day)
        return int(dt.timestamp() * 1_000_000)
    raise ValueError(f'failed to parse timestamp: {value}')


def parse_epoch_timestamp_usec(value):
    if not re.match(r'^\d+(\.\d+)?$', value):
        raise ValueError(f'failed to parse timestamp: @{value}')
    whole, _, frac = value.partition('.')
    frac = (frac + '000000')[:6]
    return int(whole) * 1_000_000 + int(frac or '0')


def parse_duration_usec(value):
    units = {
        'us': 1, 'usec': 1, 'usecs': 1,
        'ms': 1_000, 'msec': 1_000, 'msecs': 1_000,
        's': 1_000_000, 'sec': 1_000_000, 'secs': 1_000_000, 'second': 1_000_000, 'seconds': 1_000_000,
        'm': 60_000_000, 'min': 60_000_000, 'mins': 60_000_000, 'minute': 60_000_000, 'minutes': 60_000_000,
        'h': 3_600_000_000, 'hr': 3_600_000_000, 'hour': 3_600_000_000, 'hours': 3_600_000_000,
        'd': 86_400_000_000, 'day': 86_400_000_000, 'days': 86_400_000_000,
        'w': 604_800_000_000, 'week': 604_800_000_000, 'weeks': 604_800_000_000,
    }
    total = 0
    pos = 0
    for match in re.finditer(r'\s*(\d+(?:\.\d+)?)(?:\s*([A-Za-z]+))?', value):
        if match.start() != pos:
            raise ValueError(f'failed to parse duration: {value}')
        number = float(match.group(1))
        unit = (match.group(2) or 's').lower()
        if unit not in units:
            raise ValueError(f'failed to parse duration: {value}')
        total += int(number * units[unit])
        pos = match.end()
    if pos != len(value) or total == 0:
        raise ValueError(f'failed to parse duration: {value}')
    return total


def collect_boots(journal):
    boots = {}
    SdJournalSeekHead(journal)
    while True:
        rc = SdJournalNext(journal)
        if rc == 0:
            break
        entry = SdJournalGetEntry(journal)
        if not entry:
            continue
        boot_id = entry.get('boot_id')
        if isinstance(boot_id, (bytes, bytearray)):
            boot_id = boot_id.hex()
        else:
            boot_id = str(boot_id or '')
        if not boot_id or set(boot_id) == {'0'}:
            continue
        realtime = int(entry.get('realtime') or 0)
        item = boots.get(boot_id)
        if item is None:
            boots[boot_id] = {'boot_id': boot_id, 'first_entry': realtime, 'last_entry': realtime}
        else:
            item['first_entry'] = min(item['first_entry'], realtime)
            item['last_entry'] = max(item['last_entry'], realtime)
    result = sorted(boots.values(), key=lambda b: (b['first_entry'], b['boot_id']))
    base = 1 - len(result)
    for i, boot in enumerate(result):
        boot['index'] = base + i
    return result


def resolve_boot_id(journal, descriptor):
    if descriptor is None:
        return None
    descriptor = descriptor.strip()
    if descriptor == 'all':
        return None

    boot_id, offset = parse_boot_descriptor(descriptor)
    boots = collect_boots(journal)
    if not boots:
        raise ValueError('no journal boot entry found for the specified boot')

    if boot_id:
        base = next((i for i, boot in enumerate(boots) if boot['boot_id'] == boot_id), -1)
        if base < 0:
            raise ValueError(f'no journal boot entry found for the specified boot ({boot_id}{offset:+d})')
        target = base + offset
    elif offset > 0:
        target = offset - 1
    else:
        target = len(boots) - 1 + offset

    if target < 0 or target >= len(boots):
        label = f'{boot_id if boot_id else ""}{offset:+d}'
        raise ValueError(f'no journal boot entry found for the specified boot ({label})')
    return boots[target]['boot_id']


def parse_boot_descriptor(descriptor):
    if descriptor == '':
        return '', 0
    match = re.match(
        r'^(([0-9A-Fa-f]{32})|([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}))?([+-]?\d+)?$',
        descriptor,
    )
    if not match:
        raise ValueError(f'failed to parse boot descriptor: {descriptor}')
    boot_id = (match.group(1) or '').replace('-', '').lower()
    offset_text = match.group(4)
    if offset_text is None or offset_text == '':
        offset = 0
    else:
        offset = int(offset_text, 10)
    return boot_id, offset


def configure_output_mode(journal, mode):
    output_mode = OUTPUT_MODE_DEFAULT
    if mode == 'json':
        output_mode = OUTPUT_MODE_JSON
    elif mode == 'export':
        output_mode = OUTPUT_MODE_EXPORT
    journal.set_output_mode(output_mode)


def open_filtered_journal(path, args):
    journal = SdJournalOpen(path, 0)
    try:
        if args.boot is not None and args.boot.strip() != 'all':
            boot_id = resolve_boot_id(journal, args.boot)
            if boot_id:
                SdJournalAddMatch(journal, f'_BOOT_ID={boot_id}'.encode('ascii'))
                SdJournalAddConjunction(journal)

        for arg in args.positional:
            if arg == '+':
                SdJournalAddDisjunction(journal)
            elif '=' in arg:
                SdJournalAddMatch(journal, arg.encode('latin1'))

        configure_output_mode(journal, args.output)
        return journal
    except Exception:
        journal.close()
        raise


def entry_in_time_range(entry, since_usec, until_usec):
    realtime = int(entry.get('realtime') or 0)
    if since_usec is not None and realtime < since_usec:
        return False
    if until_usec is not None and realtime > until_usec:
        return False
    return True


def write_processed_output(journal, entry):
    out = SdJournalProcessOutput(journal, entry)
    if isinstance(out, bytes):
        sys.stdout.buffer.write(out)
    else:
        sys.stdout.write(out)
    sys.stdout.flush()


def iter_matching_entries(journal, since_usec, until_usec):
    if since_usec is not None:
        SdJournalSeekRealtimeUsec(journal, since_usec)
    else:
        SdJournalSeekHead(journal)
    while True:
        rc = SdJournalNext(journal)
        if rc == 0:
            break
        entry = SdJournalGetEntry(journal)
        if not entry:
            continue
        realtime = int(entry.get('realtime') or 0)
        if until_usec is not None and realtime > until_usec:
            break
        if entry_in_time_range(entry, since_usec, until_usec):
            yield entry


def show_forward(journal, head_limit, since_usec, until_usec):
    count = 0
    for entry in iter_matching_entries(journal, since_usec, until_usec):
        if head_limit > 0 and count >= head_limit:
            break
        write_processed_output(journal, entry)
        count += 1


def show_tail(journal, tail_limit, since_usec, until_usec):
    outputs = []
    for entry in iter_matching_entries(journal, since_usec, until_usec):
        outputs.append(SdJournalProcessOutput(journal, entry))
    for out in outputs[-tail_limit:]:
        if isinstance(out, bytes):
            sys.stdout.buffer.write(out)
        else:
            sys.stdout.write(out)
    sys.stdout.flush()


def scan_follow_snapshot(path, args, since_usec, until_usec):
    try:
        journal = open_filtered_journal(path, args)
    except Exception:
        return []
    try:
        entries = []
        for entry in iter_matching_entries(journal, since_usec, until_usec):
            cursor = entry.get('cursor')
            if not cursor:
                continue
            entries.append((cursor, SdJournalProcessOutput(journal, entry)))
        return entries
    finally:
        journal.close()


def run_follow(path, args, since_usec, until_usec, tail_limit):
    seen = set()
    initial = scan_follow_snapshot(path, args, since_usec, until_usec)
    for cursor, _ in initial:
        seen.add(cursor)

    if args.no_tail or since_usec is not None:
        to_print = initial
    else:
        to_print = initial[-tail_limit:]
    for _, out in to_print:
        if isinstance(out, bytes):
            sys.stdout.buffer.write(out)
        else:
            sys.stdout.write(out)
    sys.stdout.flush()

    while True:
        time.sleep(0.1)
        snapshot = scan_follow_snapshot(path, args, since_usec, until_usec)
        for cursor, out in snapshot:
            if cursor in seen:
                continue
            seen.add(cursor)
            if isinstance(out, bytes):
                sys.stdout.buffer.write(out)
            else:
                sys.stdout.write(out)
            sys.stdout.flush()


def run_verify(input_path, verify_key):
    has_verify_key = verify_key is not None
    if has_verify_key and not valid_verification_key(verify_key):
        sys.stderr.write('Failed to parse seed.\n')
        return 1

    directory_input = os.path.isdir(input_path)
    if directory_input:
        files = _collect_journal_files(input_path)
    else:
        files = [input_path]

    if not files:
        if directory_input:
            return 0
        sys.stderr.write('Error: verify: no journal files found\n')
        return 1

    first_err = None
    for path in files:
        r = None
        try:
            r = FileReader.open(path)
            sealed = (r.header()['compatible_flags'] & COMPATIBLE_SEALED) != 0
        except Exception as err:
            if directory_input:
                continue
            sys.stderr.write(f'FAIL: {path} ({err})\n')
            if first_err is None:
                first_err = err
            continue
        finally:
            if r is not None:
                r.close()

        if sealed and has_verify_key:
            try:
                verify_file_with_key(path, verify_key)
                sys.stderr.write(f'PASS: {path}\n')
            except VerificationError as err:
                sys.stderr.write(f'FAIL: {path} ({err})\n')
                if first_err is None:
                    first_err = err
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
    end_i, ok = consume_hex(key, next_i + 1)
    if not ok or end_i != len(key):
        return False
    return any(ch != '0' for ch in key[next_i + 1:end_i])


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
    parser.add_argument('--tail', help='show last N entries')
    parser.add_argument('--follow', action='store_true', help='follow appended entries')
    parser.add_argument('--sync', action='store_true', help='unsupported')
    parser.add_argument('--flush', action='store_true', help='unsupported')
    parser.add_argument('--rotate', action='store_true', help='unsupported')
    parser.add_argument('--relinquish-var', action='store_true', help='unsupported')
    parser.add_argument('--verify', action='store_true', help='verify journal file')
    parser.add_argument('--verify-only', action='store_true', help='verify only')
    parser.add_argument('--verify-key', help='FSS verification key')
    parser.add_argument('-b', '--boot', nargs='?', const='', help='filter by boot ID, offset, or all')
    parser.add_argument('-S', '--since', help='show entries not older than timestamp')
    parser.add_argument('-U', '--until', help='show entries not newer than timestamp')
    parser.add_argument('--no-tail', action='store_true')
    parser.add_argument('positional', nargs='*', help='match expressions or +')

    args = parser.parse_args(preprocess_optional_boot_args(sys.argv[1:]))

    if args.sync:
        unsupported('sync')
    if args.flush:
        unsupported('flush')
    if args.rotate:
        unsupported('rotate')
    if args.relinquish_var:
        unsupported('relinquish-var')
    path = args.file or args.directory
    if not path:
        sys.stderr.write('Error: use --file or --directory\n')
        sys.exit(1)

    if args.verify or args.verify_only or args.verify_key is not None:
        return run_verify(path, args.verify_key)

    head_limit = parse_limit('head', args.head)
    tail_limit = parse_limit('tail', args.tail) if args.tail is not None else 0
    try:
        since_usec = parse_timestamp_usec(args.since) if args.since else None
        until_usec = parse_timestamp_usec(args.until) if args.until else None
    except ValueError as err:
        sys.stderr.write(f'Error: {err}\n')
        sys.exit(1)
    if since_usec is not None and until_usec is not None and since_usec > until_usec:
        sys.stderr.write('Error: --since= must be before --until=.\n')
        sys.exit(1)

    try:
        if args.follow:
            follow_tail = tail_limit if args.tail is not None else 10
            run_follow(path, args, since_usec, until_usec, follow_tail)
            return 0

        journal = open_filtered_journal(path, args)

        if args.list_boots:
            boots = SdJournalListBoots(journal)
            for boot in boots:
                first = boot['first_entry'] // 1000000
                last = boot['last_entry'] // 1000000
                idx = str(boot['index']).rjust(4)
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
            show_tail(journal, tail_limit, since_usec, until_usec)
            journal.close()
            return 0

        show_forward(journal, head_limit, since_usec, until_usec)
        journal.close()
        return 0
    except Exception as e:
        sys.stderr.write(f'Error: {e}\n')
        sys.exit(1)


if __name__ == '__main__':
    sys.exit(main())
