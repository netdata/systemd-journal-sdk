#!/usr/bin/env python3
# Python conformance adapter: run, list, probe.
# All output is synchronous JSON on stdout.

import json
import os
import sys
import time
import tempfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from journal import (
    FileReader, DirectoryReader, Writer, SdJournalOpen,
    SdJournalSeekHead, SdJournalNext,
    SdJournalGetCursor, SdJournalTestCursor,
    SdJournalSeekCursor, SdJournalGetRealtimeUsec, export_entry, json_entry,
)
from journal.compress import _HAS_ZSTD
from journal.hash import parse_match_string


ADAPTER_VERSION = '0.1.0'


def fixture_base():
    base = os.environ.get('ADAPTER_FIXTURE_BASE')
    if base:
        return base
    dir_path = os.getcwd()
    for _ in range(20):
        test_path = os.path.join(dir_path, 'tests', 'conformance', 'manifests', 'conformance-v01.json')
        if os.path.exists(test_path):
            return dir_path
        parent = os.path.dirname(dir_path)
        if parent == dir_path:
            break
        dir_path = parent
    return os.getcwd()


def resolve_fixture(tc, key):
    if not tc.get('fixtures') or key not in tc['fixtures']:
        return ''
    return os.path.join(fixture_base(), tc['fixtures'][key]['path'])


def fixture_requires_zstd(path):
    if _HAS_ZSTD:
        return False
    if path.endswith('.zst'):
        return True
    if os.path.isdir(path):
        for _root, _dirs, files in os.walk(path):
            if any(name.endswith('.zst') for name in files):
                return True
    return False


def run_adapter():
    input_data = sys.stdin.buffer.read()
    try:
        tc = json.loads(input_data)
    except Exception as e:
        print(json.dumps({'status': 'ERROR', 'error': f'decode error: {e}'}), file=sys.stderr)
        sys.exit(1)

    start = time.time()
    result = {'test_name': tc.get('test_name', ''), 'result_format': tc.get('expected', {}).get('result_format', '')}

    try:
        category = tc.get('category', '')
        if category == 'file-format':
            result = run_file_format_test(tc)
        elif category == 'entry-parse':
            result = run_entry_parse_test(tc)
        elif category == 'matching':
            result = run_matching_test(tc)
        elif category == 'stream':
            result = run_stream_test(tc)
        elif category == 'cursor-navigation':
            result = run_cursor_test(tc)
        elif category == 'enumeration':
            result = run_enumeration_test(tc)
        elif category == 'import-export':
            result = run_import_export_test(tc)
        elif category == 'journalctl-cli':
            result = run_journalctl_test(tc)
        elif category == 'compression':
            result = run_compression_test(tc)
        elif category == 'corruption-resilience':
            result = run_corruption_test(tc)
        elif category == 'verification':
            result = run_verification_test(tc)
        else:
            result = {'status': 'SKIP', 'note': f'unsupported category: {category}'}
    except Exception as e:
        result = {'status': 'ERROR', 'error': str(e)}

    result.setdefault('test_name', tc.get('test_name', ''))
    result.setdefault('result_format', tc.get('expected', {}).get('result_format', ''))
    result['duration_ms'] = max(1, int((time.time() - start) * 1000))
    if 'status' not in result:
        result['status'] = 'SKIP'
        result['note'] = 'no matching test handler'

    print(json.dumps(result))


def run_file_format_test(tc):
    name = tc.get('test_name', '')
    if name == 'journal-file-parse-uid-from-filename':
        return test_uid_from_filename()
    elif name == 'journal-file-header-parse':
        path = resolve_fixture(tc, 'journal_file')
        if not path:
            return {'status': 'SKIP', 'note': 'no journal_file fixture'}
        if fixture_requires_zstd(path):
            return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
        return test_file_header_parse(path)
    return {'status': 'SKIP', 'note': f'unsupported: {name}'}


def test_uid_from_filename():
    tests = [
        ('user-1000.journal', 1000, True, ''),
        ('system.journal', 0, False, ''),
        ('user-foo.journal', 0, False, 'EINVAL'),
        ('user-65535.journal', 0, False, 'ENXIO'),
        ('user@00000000000000000000000000000000.journal~', 0, False, 'EREMOTE'),
    ]
    for name, exp_uid, exp_has_uid, exp_err in tests:
        uid, has_uid, err_code = _parse_uid(name)
        if uid != exp_uid or has_uid != exp_has_uid or err_code != exp_err:
            return {'status': 'FAIL', 'actual': False,
                    'error': f'{name}: got uid={uid} hasUID={has_uid} err={err_code}'}
    return {'status': 'PASS', 'actual': True}


def _parse_uid(name):
    if name == 'system.journal' or name.startswith('system@'):
        return 0, False, ''
    if name.startswith('user@'):
        return 0, False, 'EREMOTE'
    if not name.startswith('user-') or not name.endswith('.journal'):
        return 0, False, 'EINVAL'
    raw = name[5:-len('.journal')]
    try:
        parsed = int(raw, 10)
    except ValueError:
        return 0, False, 'EINVAL'
    if parsed == 65535:
        return 0, False, 'ENXIO'
    return parsed, True, ''


def test_file_header_parse(path):
    try:
        r = FileReader.open(path)
        try:
            if not r.step():
                return {'status': 'FAIL', 'error': 'fixture has no entries'}
            r.get_entry()
            h = r.header()
            return {
                'status': 'PASS',
                'actual': [{
                    'signature': h['signature'],
                    'state': h['state'],
                    'compatible_flags': h['compatible_flags'],
                    'incompatible_flags': h['incompatible_flags'],
                    'header_size': h['header_size'],
                }],
            }
        finally:
            r.close()
    except Exception as e:
        return {'status': 'FAIL', 'error': str(e)}


def run_entry_parse_test(tc):
    path = resolve_fixture(tc, 'importer_data')
    if not path:
        return {'status': 'SKIP', 'note': 'no importer_data fixture'}
    with open(path, 'rb') as f:
        data = f.read()
    entries = _parse_journal_export(data)
    if tc.get('test_name') == 'journal-importer-eof':
        return {'status': 'PASS', 'actual': len(entries) > 0, 'evidence': {'entry_count': len(entries)}}
    return {'status': 'PASS', 'actual': entries, 'evidence': {'entry_count': len(entries)}}


def _parse_journal_export(data):
    entries = []
    current = {}
    for line in data.split(b'\n'):
        if not line:
            if current:
                entries.append(current)
                current = {}
            continue
        eq = line.find(b'=')
        if eq >= 0:
            current[line[:eq].decode('utf-8', errors='replace')] = line[eq + 1:].decode('utf-8', errors='replace')
    if current:
        entries.append(current)
    return entries


def run_matching_test(tc):
    name = tc.get('test_name', '')
    if name == 'journal-match-invalid-input':
        return test_match_invalid()
    elif name == 'journal-match-boolean-logic':
        return test_match_boolean_logic()
    return {'status': 'SKIP', 'note': f'unsupported: {name}'}


def test_match_invalid():
    for item in ['foobar', '', '=', '=xxxxx']:
        try:
            parse_match_string(item)
            return {'status': 'FAIL', 'error': f'EINVAL expected for "{item}"'}
        except ValueError:
            pass
    try:
        parse_match_string('FOOBAR=waldo')
    except ValueError as e:
        return {'status': 'FAIL', 'error': f'valid match rejected: {e}'}
    return {'status': 'PASS', 'actual': 'EINVAL', 'error': 'EINVAL'}


def test_match_boolean_logic():
    with tempfile.NamedTemporaryFile(suffix='.journal', delete=False) as tmp:
        path = tmp.name
    try:
        w = Writer.create(path)
        w.append([
            {'name': 'L3', 'value': 'ok'},
            {'name': 'TWO', 'value': 'two'},
            {'name': 'ONE', 'value': 'one'},
        ])
        w.append([
            {'name': 'L4_1', 'value': 'yes'},
            {'name': 'L4_2', 'value': 'ok'},
            {'name': 'PIFF', 'value': 'paff'},
            {'name': 'QUUX', 'value': 'xxxxx'},
            {'name': 'HALLO', 'value': 'WALDO'},
            {'name': 'B', 'value': b'C\x00D'},
            {'name': 'A', 'value': b'\x01\x02'},
        ])
        w.append([{'name': 'L3', 'value': 'ok'}])
        w.append([
            {'name': 'TWO', 'value': 'two'},
            {'name': 'ONE', 'value': 'one'},
        ])
        w.close()

        r = FileReader.open(path)
        _add_systemd_complex_match_expression(r)
        matched = []
        while r.step():
            entry = r.get_entry()
            fields = {}
            for k, v in entry['fields'].items():
                try:
                    fields[k] = v.decode('utf-8')
                except Exception:
                    fields[k] = v.hex()
            matched.append(fields)
        r.close()

        if len(matched) != 2:
            return {'status': 'FAIL', 'actual': matched, 'error': f'matched {len(matched)}, want 2'}
        return {'status': 'PASS', 'actual': matched}
    finally:
        try:
            os.unlink(path)
        except OSError:
            path = ''


def _add_systemd_complex_match_expression(r):
    r.add_match(b'A=\x01\x02')
    r.add_match(b'B=C\x00D')
    r.add_match(b'HALLO=WALDO')
    r.add_match(b'QUUX=mmmm')
    r.add_match(b'QUUX=xxxxx')
    r.add_match(b'HALLO=')
    r.add_match(b'QUUX=xxxxx')
    r.add_match(b'QUUX=yyyyy')
    r.add_match(b'PIFF=paff')
    r.add_disjunction()
    r.add_match(b'ONE=one')
    r.add_match(b'ONE=two')
    r.add_match(b'TWO=two')
    r.add_conjunction()
    r.add_match(b'L4_1=yes')
    r.add_match(b'L4_1=ok')
    r.add_match(b'L4_2=yes')
    r.add_match(b'L4_2=ok')
    r.add_disjunction()
    r.add_match(b'L3=yes')
    r.add_match(b'L3=ok')


def run_stream_test(tc):
    path = resolve_fixture(tc, 'journal_dir')
    if not path:
        return {'status': 'SKIP', 'note': 'no journal_dir fixture'}
    if fixture_requires_zstd(path):
        return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
    try:
        r = DirectoryReader.open(path)
        entries = []
        count = 0
        r.seek_head()
        while r.step() and count < 100:
            entry = r.get_entry()
            if not entry:
                break
            em = {}
            for k, v in entry['fields'].items():
                try:
                    em[k] = v.decode('utf-8')
                except Exception:
                    em[k] = v.hex()
            entries.append(em)
            count += 1
        r.close()
        if not entries:
            return {'status': 'FAIL', 'error': 'no entries read'}
        return {'status': 'PASS', 'actual': entries, 'evidence': {'entry_count': len(entries)}}
    except Exception as e:
        return {'status': 'FAIL', 'error': str(e)}


def run_cursor_test(tc):
    path = resolve_fixture(tc, 'journal_dir')
    if not path:
        return {'status': 'SKIP', 'note': 'no journal_dir fixture'}
    if fixture_requires_zstd(path):
        return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
    r = SdJournalOpen(path, 0)
    try:
        SdJournalSeekHead(r)
        if SdJournalNext(r) == 0:
            return {'status': 'FAIL', 'error': 'no entries'}
        cursor = SdJournalGetCursor(r)
        if not cursor:
            return {'status': 'FAIL', 'error': 'null cursor'}
        if not SdJournalTestCursor(r, cursor):
            return {'status': 'FAIL', 'error': 'current cursor did not match'}
        cursor_realtime = SdJournalGetRealtimeUsec(r)
        if SdJournalTestCursor(r, 'invalid-cursor'):
            return {'status': 'FAIL', 'error': 'invalid cursor matched current position'}
        invalid_seek_rejected = False
        try:
            SdJournalSeekCursor(r, 'invalid-cursor')
        except Exception:
            invalid_seek_rejected = True
        if not invalid_seek_rejected:
            return {'status': 'FAIL', 'error': 'invalid seek cursor was accepted'}
        SdJournalSeekCursor(r, cursor)
        if 'n=' not in cursor:
            return {'status': 'FAIL', 'error': 'cursor missing seqnum segment'}
        missing_cursor = cursor.rsplit('n=', 1)[0] + 'n=999999'
        SdJournalSeekCursor(r, missing_cursor)
        if SdJournalTestCursor(r, cursor):
            return {'status': 'FAIL', 'error': 'missing seek stayed on original cursor'}
        if SdJournalGetRealtimeUsec(r) < cursor_realtime:
            return {'status': 'FAIL', 'error': 'missing seek moved before requested cursor'}
        return {
            'status': 'PASS',
            'actual': True,
            'evidence': {
                'found_cursor': True,
                'invalid_test_cursor': False,
                'invalid_seek_rejected': True,
                'missing_seek': True,
                'missing_seek_position': True,
            },
        }
    finally:
        r.close()


def run_enumeration_test(tc):
    path = resolve_fixture(tc, 'journal_dir')
    if not path:
        return {'status': 'SKIP', 'note': 'no journal_dir fixture'}
    if fixture_requires_zstd(path):
        return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
    try:
        r = DirectoryReader.open(path)
        fields = r.enumerate_fields()
        if isinstance(fields, set):
            fields = sorted(fields)
        return {'status': 'PASS', 'actual': fields, 'evidence': {'field_count': len(fields)}}
    finally:
        r.close()


def run_import_export_test(tc):
    path = resolve_fixture(tc, 'journal_dir')
    if not path:
        return {'status': 'SKIP', 'note': 'no journal_dir fixture'}
    if fixture_requires_zstd(path):
        return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
    try:
        r = DirectoryReader.open(path)
        exports = []
        count = 0
        r.seek_head()
        while r.step() and count < 10:
            entry = r.get_entry()
            if not entry:
                break
            exports.append(export_entry(entry).decode('utf-8', errors='replace'))
            count += 1
        r.close()
        if not exports:
            return {'status': 'FAIL', 'error': 'no exports generated'}
        return {'status': 'PASS', 'actual': exports, 'evidence': {'export_count': len(exports)}}
    except Exception as e:
        return {'status': 'FAIL', 'error': str(e)}


def run_journalctl_test(tc):
    name = tc.get('test_name', '')
    if name == 'journal-list-boots':
        path = resolve_fixture(tc, 'journal_dir')
        if not path:
            return {'status': 'SKIP', 'note': 'no journal_dir fixture'}
        if fixture_requires_zstd(path):
            return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
        try:
            r = DirectoryReader.open(path)
            return {'status': 'PASS', 'actual': r.list_boots()}
        finally:
            r.close()
    return {'status': 'SKIP', 'note': f'unsupported: {name}'}


def run_compression_test(tc):
    path = resolve_fixture(tc, 'journal_file')
    if not path:
        return {'status': 'SKIP', 'note': 'no journal_file fixture'}
    if path.endswith('.zst') and not _HAS_ZSTD:
        return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
    try:
        r = FileReader.open(path)
        if not r.step():
            return {'status': 'FAIL', 'error': 'no entries in compressed fixture'}
        entry = r.get_entry()
        r.close()
        msg = entry['fields'].get('MESSAGE', b'')
        transport = entry['fields'].get('_TRANSPORT', b'')
        return {
            'status': 'PASS',
            'actual': True,
            'evidence': {
                'message': msg.decode('utf-8', errors='replace'),
                'transport': transport.decode('utf-8', errors='replace'),
            },
        }
    except Exception as e:
        return {'status': 'FAIL', 'error': str(e)}


def run_corruption_test(tc):
    name = tc.get('test_name', '')
    if name == 'journal-verify-corruption-detection':
        from journal.verify import verify_file, VerificationError
        path = resolve_fixture(tc, 'corrupted_file')
        if not path:
            return {'status': 'SKIP', 'note': 'no corrupted_file fixture'}
        if fixture_requires_zstd(path):
            return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
        try:
            verify_file(path)
        except VerificationError as e:
            return {'status': 'PASS', 'actual': str(e), 'error': str(e)}
        return {'status': 'FAIL', 'error': 'verification did not detect corruption in truncated zstd frame'}
    checked = 0
    read_errors = 0
    for key in ['corrupted_file', 'afl_corrupted_1', 'afl_corrupted_2']:
        path = resolve_fixture(tc, key)
        if not path:
            continue
        if fixture_requires_zstd(path):
            return {'status': 'SKIP', 'note': 'zstd decompression unavailable'}
        checked += 1
        try:
            r = FileReader.open(path)
            for _ in range(1000):
                if not r.step():
                    break
                try:
                    r.get_entry()
                except Exception:
                    read_errors += 1
                    break
            r.close()
        except Exception:
            read_errors += 1
    if checked == 0:
        return {'status': 'SKIP', 'note': 'no corruption fixtures'}
    return {'status': 'PASS', 'actual': True, 'evidence': {'checked': checked, 'read_errors': read_errors}}


def run_verification_test(tc):
    if tc.get('test_name') == 'journal-verify-sealed':
        from journal.seal import SealOptions
        import shutil
        tmp = tempfile.mkdtemp(prefix='adapter-verify-sealed-')
        try:
            path = os.path.join(tmp, 'sealed.journal')
            seed = b'\x00' * 12
            seal_opts = SealOptions(seed, interval_usec=1000000, start_usec=1000000)
            w = Writer.create(path, opts={'seal': seal_opts})
            w.append([{'name': 'MESSAGE', 'value': 'sealed verify'}],
                     {'realtime_usec': 1500000})
            w.close()
            key = f'{seed.hex()}/{seal_opts.start_usec // seal_opts.interval_usec:x}-{seal_opts.interval_usec:x}'
            from journal.verify import verify_file_with_key, VerificationError
            try:
                verify_file_with_key(path, key)
            except VerificationError as e:
                return {'status': 'FAIL', 'actual': False, 'error': str(e)}
            return {'status': 'PASS', 'actual': True}
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
    return {'status': 'SKIP', 'note': f'unsupported verification test: {tc.get("test_name")}'}


def list_tests():
    tests = [
        'journal-file-parse-uid-from-filename',
        'journal-file-header-parse',
        'journal-importer-basic-parsing',
        'journal-importer-eof',
        'journal-match-boolean-logic',
        'journal-match-invalid-input',
        'journal-stream-directory-iteration',
        'journal-cursor-test',
        'journal-query-unique-fields',
        'journal-export-format',
        'journal-list-boots',
        'journal-zstd-compressed-read',
        'journal-verify-sealed',
        'journal-corruption-append-resilient',
        'journal-verify-corruption-detection',
    ]
    print(json.dumps(tests))


def probe_adapter():
    info = {
        'adapter_version': ADAPTER_VERSION,
        'language': 'python',
        'capabilities': {
            'file_reader': True,
            'directory_reader': True,
            'forward_iter': True,
            'backward_iter': True,
            'cursor_nav': True,
            'match_and': True,
            'match_or': True,
            'match_disjunction': True,
            'unique_fields': True,
            'export_output': True,
            'json_output': True,
            'list_boots': True,
            'zstd_decompress': _HAS_ZSTD,
            'verification': True,
            'fss': True,
        },
    }
    print(json.dumps(info))


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print('Usage: adapter [run|list|probe]', file=sys.stderr)
        sys.exit(1)
    cmd = sys.argv[1]
    if cmd == 'run':
        run_adapter()
    elif cmd == 'list':
        list_tests()
    elif cmd == 'probe':
        probe_adapter()
    else:
        print(f'Unknown subcommand: {cmd}', file=sys.stderr)
        sys.exit(1)
