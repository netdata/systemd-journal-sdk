#!/usr/bin/env python3
"""Live writer for the shared concurrency harness."""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from journal import Writer


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
    parser.add_argument(
        '--compression-threshold-bytes',
        '--compress-threshold',
        dest='compression_threshold_bytes',
        type=parse_positive_int,
        default=64,
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
    os.makedirs(os.path.dirname(args.path), exist_ok=True)
    os.makedirs(os.path.dirname(args.ready_file), exist_ok=True)

    writer = Writer.create(args.path, {
        'compression': args.compression,
        'compression_threshold_bytes': args.compression_threshold_bytes,
        'compact': args.compact,
    })
    try:
        realtime_base = 1_700_001_000_000_000
        for seq in range(args.entries):
            if args.binary_fixture and seq == 0:
                fields = [
                    {'name': 'TEST_ID', 'value': b'binary-interoperability'},
                    {'name': 'MESSAGE', 'value': b'binary interoperability'},
                    {'name': 'PRIORITY', 'value': b'6'},
                    {'name': 'LIVE_SEQ', 'value': b'000000'},
                    {'name': 'BINARY_PAYLOAD', 'value': bytes([0x00, 0x01, 0x02, 0x41, 0x0a, 0x7f, 0x80, 0xff])},
                    {'name': 'BINARY_MATCH', 'value': bytes([0x61, 0x62, 0x63, 0x07, 0x64, 0x65, 0x66])},
                    {'name': 'BINARY_EMPTY', 'value': b''},
                ]
            elif args.zstd_fixture and seq == 0:
                large_payload = bytes([(i % 26) + 0x41 for i in range(256)])
                fields = [
                    {'name': 'TEST_ID', 'value': b'zstd-interoperability'},
                    {'name': 'MESSAGE', 'value': b'zstd interoperability'},
                    {'name': 'PRIORITY', 'value': b'6'},
                    {'name': 'LIVE_SEQ', 'value': b'000000'},
                    {'name': 'COMPRESSED_PAYLOAD', 'value': large_payload},
                    {'name': 'COMPRESSED_MATCH', 'value': large_payload[:32]},
                ]
            elif args.xz_fixture and seq == 0:
                large_payload = bytes([(i % 26) + 0x41 for i in range(256)])
                fields = [
                    {'name': 'TEST_ID', 'value': b'xz-interoperability'},
                    {'name': 'MESSAGE', 'value': b'xz interoperability'},
                    {'name': 'PRIORITY', 'value': b'6'},
                    {'name': 'LIVE_SEQ', 'value': b'000000'},
                    {'name': 'COMPRESSED_PAYLOAD', 'value': large_payload},
                    {'name': 'COMPRESSED_MATCH', 'value': large_payload[:32]},
                ]
            elif args.lz4_fixture and seq == 0:
                large_payload = bytes([(i % 26) + 0x41 for i in range(256)])
                fields = [
                    {'name': 'TEST_ID', 'value': b'lz4-interoperability'},
                    {'name': 'MESSAGE', 'value': b'lz4 interoperability'},
                    {'name': 'PRIORITY', 'value': b'6'},
                    {'name': 'LIVE_SEQ', 'value': b'000000'},
                    {'name': 'COMPRESSED_PAYLOAD', 'value': large_payload},
                    {'name': 'COMPRESSED_MATCH', 'value': large_payload[:32]},
                ]
            else:
                fields = [
                    {'name': 'MESSAGE', 'value': f'live-{seq:06d}'},
                    {'name': 'PRIORITY', 'value': b'6'},
                    {'name': 'SYSLOG_IDENTIFIER', 'value': b'python-live-writer'},
                    {'name': 'LIVE_SEQ', 'value': f'{seq:06d}'},
                ]

            writer.append(fields, {
                'realtime_usec': realtime_base + seq,
                'monotonic_usec': seq + 1,
            })

            if seq == 0:
                writer.sync()
                with open(args.ready_file, 'w', encoding='utf-8') as f:
                    f.write('ready\n')
            elif args.sync_every > 0 and (seq + 1) % args.sync_every == 0:
                writer.sync()

            if args.crash_after > 0 and seq + 1 >= args.crash_after:
                os._exit(17)

            if delay > 0:
                time.sleep(delay)

        writer.close()
    except Exception as e:
        try:
            writer.close()
        except Exception:
            pass
        print(str(e), file=sys.stderr)
        sys.exit(1)

    sys.exit(0)


if __name__ == '__main__':
    main()
