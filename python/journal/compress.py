# Compression support and journal file name helpers.

import lzma
import os
import tempfile
import shutil

try:
    import compression.zstd
    _HAS_ZSTD = True
except ImportError:
    _HAS_ZSTD = False

try:
    import lz4.block
    _HAS_LZ4 = True
except ImportError:
    _HAS_LZ4 = False

MAX_UNCOMPRESSED_SIZE = 768 * 1024 * 1024  # 768 MiB cap matching Rust/Go


def decompress_zst_sync(input_data):
    if _HAS_ZSTD:
        if isinstance(input_data, (bytes, bytearray, memoryview)):
            return compression.zstd.decompress(input_data)
        with open(input_data, 'rb') as f:
            return compression.zstd.decompress(f.read())
    raise RuntimeError('zstd decompression not available')


def decompress_xz_sync(input_data):
    if isinstance(input_data, (bytes, bytearray, memoryview)):
        return lzma.decompress(bytes(input_data), format=lzma.FORMAT_XZ)
    with open(input_data, 'rb') as f:
        return lzma.decompress(f.read(), format=lzma.FORMAT_XZ)


def decompress_lz4_sync(input_data):
    if not _HAS_LZ4:
        raise RuntimeError('lz4 decompression not available')
    if isinstance(input_data, (bytes, bytearray, memoryview)):
        src = input_data
    else:
        with open(input_data, 'rb') as f:
            src = f.read()
    if len(src) < 8:
        raise ValueError('lz4 data too short for size prefix')
    uncompressed_size = int.from_bytes(src[:8], 'little')
    if uncompressed_size > MAX_UNCOMPRESSED_SIZE:
        raise ValueError(f'lz4 uncompressed size {uncompressed_size} exceeds cap {MAX_UNCOMPRESSED_SIZE}')
    compressed = src[8:]
    return lz4.block.decompress(compressed, uncompressed_size=uncompressed_size)


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
