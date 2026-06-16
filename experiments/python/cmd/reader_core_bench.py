#!/usr/bin/env python3
"""Python reader-core benchmark command."""

import argparse
import json
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PYTHON_ROOT = ROOT / 'python'
sys.path.insert(0, str(PYTHON_ROOT))

from journal import (  # noqa: E402
    DirectoryReader,
    FileReader,
    ReaderOptions,
    READER_ACCESS_AUTO,
    READER_ACCESS_MMAP,
    SdJournalEnumerateAvailableData,
    SdJournalNext,
    SdJournalOpenDirectory,
    SdJournalOpenFiles,
    SdJournalRestartData,
)
from journal.reader_access import READER_ACCESS_READ_AT  # noqa: E402


class Counts:
    def __init__(self):
        self.records = 0
        self.fields = 0
        self.bytes = 0
        self.checksum = 0

    def add_payload(self, payload):
        self.fields += 1
        self.bytes += len(payload)
        self.checksum = checksum_payload(self.checksum, payload)

    def add_record_marker(self, value):
        self.records += 1
        self.checksum = rotate_left(self.checksum, 7) ^ int(value)


def rotate_left(value, shift):
    value &= 0xffffffffffffffff
    return ((value << shift) | (value >> (64 - shift))) & 0xffffffffffffffff


def checksum_payload(checksum, payload):
    checksum = rotate_left(checksum, 5) ^ len(payload)
    if payload:
        checksum ^= payload[0] << 8
        checksum ^= payload[-1]
    return checksum & 0xffffffffffffffff


def process_status_kb():
    try:
        status = Path('/proc/self/status').read_text(encoding='utf-8')
    except OSError:
        return {}
    wanted = {
        'VmSize', 'VmPeak', 'VmRSS', 'VmHWM', 'RssAnon', 'RssFile',
        'RssShmem', 'VmData', 'VmStk', 'VmExe', 'VmLib', 'VmPTE',
    }
    out = {}
    for line in status.splitlines():
        key, sep, value = line.partition(':')
        if not sep or key not in wanted:
            continue
        parts = value.split()
        if parts:
            try:
                out[f'{key}_kb'] = int(parts[0])
            except ValueError:
                continue
    return out


def advance(reader, direction):
    return reader.previous() if direction == 'backward' else reader.next()


def benchmark_reader_options(args):
    access_mode = benchmark_access_mode(args.mmap_strategy)
    kwargs = {
        'access_mode': access_mode,
        'bounds': args.bounds,
    }
    window_size = int(args.window_size)
    if window_size > 0:
        kwargs['window_size'] = window_size
    return ReaderOptions(**kwargs)


def benchmark_access_mode(strategy):
    normalized = str(strategy).lower().replace('_', '-')
    aliases = {
        'auto': READER_ACCESS_AUTO,
        'mmap': READER_ACCESS_MMAP,
        'windowed': READER_ACCESS_MMAP,
        'read-at': READER_ACCESS_READ_AT,
        'readat': READER_ACCESS_READ_AT,
        'pread': READER_ACCESS_READ_AT,
    }
    try:
        return aliases[normalized]
    except KeyError as err:
        raise ValueError(f'invalid Python mmap strategy: {strategy}') from err


def open_sdk_reader(inputs, surface, options):
    if surface == 'file':
        if len(inputs) != 1:
            raise ValueError('file surface requires exactly one --input')
        return FileReader.open(inputs[0], options=options)
    if surface == 'directory':
        if len(inputs) != 1:
            raise ValueError('directory surface requires exactly one --input')
        return DirectoryReader.open(inputs[0], options=options)
    if surface == 'open-files':
        return DirectoryReader.open_files(inputs, options=options)
    raise ValueError(f'invalid surface: {surface}')


def seek_reader(reader, direction):
    if direction == 'backward':
        reader.seek_tail()
    else:
        reader.seek_head()


def read_sdk(inputs, surface, mode, direction, options):
    reader = open_sdk_reader(inputs, surface, options)
    try:
        seek_reader(reader, direction)
        counts = Counts()
        while advance(reader, direction):
            if mode == 'sdk-entry':
                entry = reader.get_entry()
                counts.add_record_marker(entry['realtime'])
                for payload in entry['payloads']:
                    counts.add_payload(payload)
            elif mode == 'sdk-payloads':
                counts.add_record_marker(reader.get_realtime_usec())
                reader.visit_entry_payloads(counts.add_payload)
            else:
                raise ValueError(f'invalid SDK mode: {mode}')
        return counts, reader.access_stats()
    finally:
        reader.close()


def open_facade(inputs, surface, options):
    if surface == 'file' or surface == 'open-files':
        return SdJournalOpenFiles(inputs, 0, options=options)
    if surface == 'directory':
        if len(inputs) != 1:
            raise ValueError('directory surface requires exactly one --input')
        return SdJournalOpenDirectory(inputs[0], 0, options=options)
    raise ValueError(f'invalid facade surface: {surface}')


def read_facade(inputs, surface, mode, direction, options):
    journal = open_facade(inputs, surface, options)
    try:
        seek_reader(journal, direction)
        counts = Counts()
        while True:
            advanced = journal.previous() if direction == 'backward' else SdJournalNext(journal)
            if advanced == 0:
                break
            if mode == 'facade-next':
                counts.add_record_marker(journal.get_realtime_usec())
            elif mode == 'facade-data':
                counts.add_record_marker(journal.get_realtime_usec())
                SdJournalRestartData(journal)
                while True:
                    payload = SdJournalEnumerateAvailableData(journal)
                    if payload is None:
                        break
                    counts.add_payload(payload)
            else:
                raise ValueError(f'invalid facade mode: {mode}')
        return counts, journal.access_stats()
    finally:
        journal.close()


def run(args):
    options = benchmark_reader_options(args)
    status_before = process_status_kb()
    started = time.perf_counter()
    if args.mode in ('sdk-entry', 'sdk-payloads'):
        counts, access_stats = read_sdk(args.inputs, args.surface, args.mode, args.direction, options)
    elif args.mode in ('facade-next', 'facade-data'):
        counts, access_stats = read_facade(args.inputs, args.surface, args.mode, args.direction, options)
    else:
        raise ValueError(f'invalid --mode for Python reader benchmark: {args.mode}')
    elapsed = time.perf_counter() - started
    status_after = process_status_kb()
    return counts, access_stats, elapsed, status_before, status_after


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--input', dest='inputs', action='append', required=True)
    parser.add_argument('--mode', default='sdk-payloads')
    parser.add_argument('--surface', default='file')
    parser.add_argument('--direction', choices=('forward', 'backward'), default='forward')
    parser.add_argument('--window-size', default='0')
    parser.add_argument('--bounds', default='live')
    parser.add_argument('--mmap-strategy', default='mmap')
    args = parser.parse_args()

    counts, access_stats, read_seconds, status_before, status_after = run(args)
    print(json.dumps({
        'language': 'python',
        'surface': args.surface,
        'mode': args.mode,
        'direction': args.direction,
        'records': counts.records,
        'fields': counts.fields,
        'bytes': counts.bytes,
        'checksum': counts.checksum,
        'read_seconds': read_seconds,
        'read_rows_per_second': counts.records / read_seconds if read_seconds > 0 else 0.0,
        'read_fields_per_second': counts.fields / read_seconds if read_seconds > 0 else 0.0,
        'read_bytes_per_second': counts.bytes / read_seconds if read_seconds > 0 else 0.0,
        'inputs': args.inputs,
        'window_size': args.window_size,
        'bounds': args.bounds,
        'mmap_strategy': args.mmap_strategy,
        'access_stats': access_stats,
        'timer_excludes': ['fixture generation', 'process startup', 'external verification'],
        'process_status_before': status_before,
        'process_status_after': status_after,
        'errors': [],
    }))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
