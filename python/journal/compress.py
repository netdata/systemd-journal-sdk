# Compression support and journal file name helpers.

import os
import tempfile
import struct
import shutil

try:
    import compression.zstd
    _HAS_ZSTD = True
except ImportError:
    _HAS_ZSTD = False


def decompress_zst_sync(input_data):
    if _HAS_ZSTD:
        if isinstance(input_data, (bytes, bytearray, memoryview)):
            return compression.zstd.decompress(input_data)
        with open(input_data, 'rb') as f:
            return compression.zstd.decompress(f.read())
    raise RuntimeError('zstd decompression not available')


def decompress_zst_to_temp(input_path, prefix='python-journal'):
    data = decompress_zst_sync(input_path)
    temp_dir = tempfile.mkdtemp(prefix=prefix + '-')
    try:
        temp_path = os.path.join(temp_dir, 'decompressed.journal')
        with open(temp_path, 'wb') as f:
            f.write(data)
        return temp_path
    except Exception:
        shutil.rmtree(temp_dir, ignore_errors=True)
        raise


def is_journal_file_name(name):
    if isinstance(name, bytes):
        name = name.decode('utf-8', errors='replace')
    return (name.endswith('.journal') or
            name.endswith('.journal~') or
            name.endswith('.journal.zst') or
            name.endswith('.journal~.zst'))


def is_zst_file(path):
    if isinstance(path, bytes):
        path = path.decode('utf-8', errors='replace')
    return path.endswith('.zst')
