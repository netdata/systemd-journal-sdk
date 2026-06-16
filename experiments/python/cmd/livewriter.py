#!/usr/bin/env python3
"""Live writer for the shared concurrency harness."""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from journal import Writer
from journal.lock import WriterLock
from journal.seal import SealOptions


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--path', required=True)
    parser.add_argument('--ready-file', required=True)
    parser.add_argument('--entries', type=parse_positive_int, required=True)
    parser.add_argument('--delay', default='1ms')
    parser.add_argument('--sync-every', type=parse_non_negative_int, default=25)
    parser.add_argument('--crash-after', type=parse_non_negative_int, default=0)
    parser.add_argument('--binary-fixture', action='store_true')
    parser.add_argument('--zstd-fixture', action='store_true')
    parser.add_argument('--xz-fixture', action='store_true')
    parser.add_argument('--lz4-fixture', action='store_true')
    parser.add_argument('--compression', default='none', choices=['none', 'zstd', 'xz', 'lz4'])
    parser.add_argument('--compact', action='store_true')
    parser.add_argument('--seal', action='store_true')
    parser.add_argument('--seal-interval-usec', type=parse_positive_int, default=1_000_000)
    parser.add_argument('--seal-start-usec', type=parse_positive_int, default=1_700_001_000_000_000)
    parser.add_argument(
        '--compression-threshold-bytes',
        '--compress-threshold',
        dest='compression_threshold_bytes',
        type=parse_positive_int,
        default=512,
    )
    return parser.parse_args(argv)


def parse_positive_int(value):
    parsed = int(value, 10)
    if parsed <= 0:
        raise argparse.ArgumentTypeError('must be positive')
    return parsed


def parse_non_negative_int(value):
    parsed = int(value, 10)
    if parsed < 0:
        raise argparse.ArgumentTypeError('must be non-negative')
    return parsed


def parse_delay_seconds(value):
    if value == '0':
        return 0.0
    units = {
        'ns': 1e-9,
        'us': 1e-6,
        'ms': 0.001,
        's': 1.0,
    }
    for suffix, factor in units.items():
        if value.endswith(suffix):
            amount = int(value[:-len(suffix)], 10)
            if amount < 0:
                raise ValueError('delay must be non-negative')
            return amount * factor
    raise ValueError(f'invalid delay: {value}')


def main():
    args = parse_args(sys.argv[1:])
    delay = parse_delay_seconds(args.delay)
    _ensure_output_dirs(args)
    lock = WriterLock.acquire(args.path)
    writer = None
    try:
        writer = Writer.create(args.path, _writer_options(args))
        _run_append_loop(writer, args, delay)
        writer.close()
    except Exception as e:
        if writer is not None:
            _best_effort(writer.close)
        _best_effort(lock.release)
        print(str(e), file=sys.stderr)
        sys.exit(1)
    finally:
        _best_effort(lock.release)

    sys.exit(0)


def _ensure_output_dirs(args):
    os.makedirs(os.path.dirname(args.path), exist_ok=True)
    os.makedirs(os.path.dirname(args.ready_file), exist_ok=True)


def _writer_options(args):
    writer_options = {
        'compression': args.compression,
        'compression_threshold_bytes': args.compression_threshold_bytes,
        'compact': args.compact,
    }
    if args.seal:
        writer_options['seal'] = SealOptions(
            seed=bytes(12),
            interval_usec=args.seal_interval_usec,
            start_usec=args.seal_start_usec,
        )
    return writer_options


def _run_append_loop(writer, args, delay):
    realtime_base = 1_700_001_000_000_000
    for seq in range(args.entries):
        writer.append(_fields_for_sequence(args, seq), {
            'realtime_usec': realtime_base + seq,
            'monotonic_usec': seq + 1,
        })
        _publish_live_progress(writer, args, seq)
        _maybe_crash(args, seq)
        if delay > 0:
            time.sleep(delay)


def _fields_for_sequence(args, seq):
    if seq == 0:
        fixture = _first_entry_fixture_fields(args)
        if fixture is not None:
            return fixture
    return [
        {'name': 'MESSAGE', 'value': f'live-{seq:06d}'},
        {'name': 'PRIORITY', 'value': b'6'},
        {'name': 'SYSLOG_IDENTIFIER', 'value': b'python-live-writer'},
        {'name': 'LIVE_SEQ', 'value': f'{seq:06d}'},
    ]


def _first_entry_fixture_fields(args):
    if args.binary_fixture:
        return _binary_fixture_fields()
    if args.zstd_fixture:
        return _compressed_fixture_fields(b'zstd-interoperability', b'zstd interoperability')
    if args.xz_fixture:
        return _compressed_fixture_fields(b'xz-interoperability', b'xz interoperability')
    if args.lz4_fixture:
        return _compressed_fixture_fields(b'lz4-interoperability', b'lz4 interoperability')
    return None


def _binary_fixture_fields():
    return [
        {'name': 'TEST_ID', 'value': b'binary-interoperability'},
        {'name': 'MESSAGE', 'value': b'binary interoperability'},
        {'name': 'PRIORITY', 'value': b'6'},
        {'name': 'LIVE_SEQ', 'value': b'000000'},
        {'name': 'BINARY_PAYLOAD', 'value': bytes([0x00, 0x01, 0x02, 0x41, 0x0a, 0x7f, 0x80, 0xff])},
        {'name': 'BINARY_MATCH', 'value': bytes([0x61, 0x62, 0x63, 0x07, 0x64, 0x65, 0x66])},
        {'name': 'BINARY_EMPTY', 'value': b''},
        {'name': 'BINARY_COMPRESSIBLE', 'value': b'A' * 256},
    ]


def _compressed_fixture_fields(test_id, message):
    large_payload = bytes([(i % 26) + 0x41 for i in range(256)])
    return [
        {'name': 'TEST_ID', 'value': test_id},
        {'name': 'MESSAGE', 'value': message},
        {'name': 'PRIORITY', 'value': b'6'},
        {'name': 'LIVE_SEQ', 'value': b'000000'},
        {'name': 'COMPRESSED_PAYLOAD', 'value': large_payload},
        {'name': 'COMPRESSED_MATCH', 'value': large_payload[:32]},
    ]


def _publish_live_progress(writer, args, seq):
    if seq == 0:
        writer.sync()
        with open(args.ready_file, 'w', encoding='utf-8') as f:
            f.write('ready\n')
    elif args.sync_every > 0 and (seq + 1) % args.sync_every == 0:
        writer.sync()


def _maybe_crash(args, seq):
    if args.crash_after > 0 and seq + 1 >= args.crash_after:
        os._exit(17)


def _best_effort(callback):
    try:
        callback()
        return True
    except Exception:
        return False


if __name__ == '__main__':
    main()
