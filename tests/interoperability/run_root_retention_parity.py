#!/usr/bin/env python3
"""Bounded synthetic Go/Rust root-retention matrix. Never opens host journals.

Run from any directory; set CARGO_HOME/GOCACHE/etc. for prewarmed offline caches.
Every SDK consumes both writers' files. Expectations come from the fixture plan,
not either implementation's output. Evidence is retained under .local/.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
DAY = 86_400_000_000
MIB = 1024 * 1024
HEADER_DAMAGE = {
    'entry-array-overflow': (176, (2**64-1).to_bytes(8, 'little')),
    'entry-array-inside-header': (176, (8).to_bytes(8, 'little')),
    'entry-array-after-tail': (176, (8*MIB-24).to_bytes(8, 'little')),
    'tail-array-outside-arena': (256, (2**32-8).to_bytes(4, 'little')),
    'tail-array-count-outside-arena': (260, (2**32-1).to_bytes(4, 'little')),
    'tail-entry-overflow': (264, (2**64-1).to_bytes(8, 'little')),
    'tail-entry-after-tail': (264, (8*MIB-64).to_bytes(8, 'little')),
}


def run(args: list[str], cwd: Path = ROOT) -> str:
    print(f"[{cwd}] {shlex.join(map(str, args))}", file=sys.stderr)
    try:
        result = subprocess.run(args, cwd=cwd, check=True, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)
    except subprocess.CalledProcessError as exc:
        print(f"command failed in {cwd}, status {exc.returncode}: {exc.stderr}", file=sys.stderr)
        raise SystemExit(exc.returncode) from exc
    return result.stdout.strip()


def snapshot(root: Path) -> dict[str, str]:
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob('*') if p.is_file()}


def row(machine: int, count: int, head: int, tail: int, active: bool = False,
        seq: int = 1) -> str:
    return f"{bytes([machine] * 16).hex()} {8*MIB} {count} {seq} {seq+count-1} {head} {tail} {str(active).lower()}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--benchmark', action='store_true',
                        help='time 1000 header inventories for 1 versus 10000 entries')
    options = parser.parse_args()
    output = ROOT / '.local/retention-parity'
    output.mkdir(parents=True, exist_ok=True)
    for key, directory in {
        'CARGO_HOME': 'cargo-home', 'CARGO_TARGET_DIR': 'cargo-target',
        'GOCACHE': 'go-build', 'GOMODCACHE': 'go-mod', 'GOPATH': 'go-path',
    }.items():
        os.environ.setdefault(key, str(output / 'caches' / directory))
    bins = {'go': output / 'go-probe', 'rust': output / 'rust-probe'}
    if not options.skip_build:
        run(['go', 'build', '-o', str(bins['go']),
             str(ROOT / 'tests/interoperability/root_retention/go_probe.go')], ROOT / 'go')
        target = Path(os.environ.get('CARGO_TARGET_DIR', str(output / 'cargo-target')))
        if not target.is_absolute():
            target = ROOT / target
        # Keep Cargo's generated lock and workspace outside committed test sources.
        source = ROOT / 'tests/interoperability/root_retention/rust'
        crate = output / 'rust-probe-src'
        (crate / 'src').mkdir(parents=True, exist_ok=True)
        manifest = (source / 'Cargo.toml').read_text().replace(
            '../../../../rust/', str(ROOT / 'rust') + '/')
        (crate / 'Cargo.toml').write_text(manifest)
        shutil.copy2(source / 'src/main.rs', crate / 'src/main.rs')
        run([options.cargo, 'build', '--offline', '--manifest-path',
             str(crate / 'Cargo.toml'), '--target-dir', str(target)])
        shutil.copy2(target / 'debug/root-retention-parity-probe', bins['rust'])
    evidence = Path(tempfile.mkdtemp(prefix='run-', dir=output))
    now = time.time_ns() // 1000 + 60_000_000  # Above either SDK's fresh clock floor.
    expected = sorted([row(21, 1, now-40*DAY, now-40*DAY),
                       row(22, 2, now-40*DAY, now-20*DAY),
                       row(23, 1, now-40*DAY, now-40*DAY, True)])
    checks = []
    def probe(lang: str, mode: str, root: Path) -> list[str]:
        return sorted(run([str(bins[lang]), mode, str(root), str(now)]).splitlines())
    for writer in bins:
        fixture = evidence / f'{writer}-fixture'
        fixture.mkdir()
        probe(writer, 'fixture', fixture)
        for reader in bins:
            assert probe(reader, 'inspect', fixture) == expected
            assert probe(reader, 'read', fixture) == sorted(map(str, [now-40*DAY]*3 + [now-20*DAY]))
            maintained = evidence / f'{writer}-maintained-by-{reader}'
            shutil.copytree(fixture, maintained)
            assert probe(reader, 'maintain', maintained) == sorted(['deleted 2', expected[1]])
            for verifier in bins:
                assert probe(verifier, 'inspect', maintained) == [expected[1]]
                assert probe(verifier, 'read', maintained) == sorted(map(str, [now-40*DAY, now-20*DAY]))
            for damage in ('malformed', 'quarantine', *HEADER_DAMAGE):
                unsafe = evidence / f'{writer}-{reader}-{damage}'
                shutil.copytree(fixture, unsafe)
                path = next(unsafe.rglob('history@*.journal'))
                if damage == 'quarantine':
                    path.rename(path.with_suffix('.journal~'))
                else:
                    with path.open('r+b') as stream:
                        offset, value = HEADER_DAMAGE.get(damage, (0, b'BROKEN!!'))
                        stream.seek(offset)
                        stream.write(value)
                before = snapshot(unsafe)
                assert probe(reader, 'reject', unsafe) == ['rejected']
                assert snapshot(unsafe) == before, 'preflight changed journal evidence'
            checks.append(f'{writer} writer / {reader} inventory, readback, age, unsafe preflight')
        live = evidence / f'{writer}-live'
        live.mkdir()
        successor = [row(1, 1, now+32*DAY, now+32*DAY, seq=2)]
        assert probe(writer, 'live', live) == successor
        for reader in bins:
            assert probe(reader, 'inspect', live) == successor
            assert probe(reader, 'read', live) == [str(now+32*DAY)]
        checks.append(f'{writer} lazy live expiry / both readers successor sequence and readback')
    timings = []
    if options.benchmark:
        for writer in bins:
            for entries in (1, 10000):
                fixture = evidence / f'{writer}-benchmark-{entries}'
                fixture.mkdir()
                run([str(bins[writer]), 'fixture', str(fixture), str(now), str(entries)])
                for reader in bins:
                    elapsed = int(probe(reader, 'benchmark', fixture)[0])
                    timings.append({'writer': writer, 'reader': reader, 'entries': entries,
                                    'mean_inventory_ns': elapsed, 'iterations': 1000,
                                    'build': 'existing binary' if options.skip_build else
                                    ('Go default optimized' if reader == 'go' else 'Rust debug')})
    report = {'passed': True, 'checks': checks, 'fixture_time_usec': now,
              'evidence': str(evidence.relative_to(ROOT)), 'timings': timings}
    (evidence / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
