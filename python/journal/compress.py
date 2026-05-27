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


def decompress_zst_sync(input_data, max_output_size=None):
    if _HAS_ZSTD:
        src = _read_compressed_input(input_data)
        if max_output_size is None:
            return compression.zstd.decompress(src)
        content_size = _zstd_frame_content_size(src)
        if content_size is not None and content_size > max_output_size:
            raise ValueError(f'zstd decompressed size {content_size} exceeds cap {max_output_size}')
        decompressor = compression.zstd.ZstdDecompressor()
        out = decompressor.decompress(src, max_output_size)
        if not decompressor.eof:
            raise ValueError(f'zstd decompressed payload exceeds cap {max_output_size}')
        return out
    raise RuntimeError('zstd decompression not available')


def decompress_xz_sync(input_data, max_output_size=MAX_UNCOMPRESSED_SIZE):
    src = _read_compressed_input(input_data)
    if max_output_size is None:
        return lzma.decompress(src, format=lzma.FORMAT_XZ)
    decompressor = lzma.LZMADecompressor(format=lzma.FORMAT_XZ)
    out = decompressor.decompress(src, max_output_size)
    if not decompressor.eof:
        raise ValueError(f'xz decompressed payload exceeds cap {max_output_size}')
    return out


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


def _read_compressed_input(input_data):
    if isinstance(input_data, (bytes, bytearray, memoryview)):
        return bytes(input_data)
    with open(input_data, 'rb') as f:
        return f.read()


def _zstd_frame_content_size(src):
    if len(src) < 6 or src[:4] != b'\x28\xb5\x2f\xfd':
        return None

    descriptor = src[4]
    fcs_flag = descriptor >> 6
    single_segment = bool(descriptor & 0x20)
    dictionary_id_flag = descriptor & 0x03
    pos = 5

    if not single_segment:
        pos += 1
    if pos > len(src):
        return None

    dict_id_sizes = (0, 1, 2, 4)
    pos += dict_id_sizes[dictionary_id_flag]
    if pos > len(src):
        return None

    if fcs_flag == 0:
        if not single_segment or pos + 1 > len(src):
            return None
        return src[pos]
    if fcs_flag == 1:
        if pos + 2 > len(src):
            return None
        return int.from_bytes(src[pos:pos + 2], 'little') + 256
    if fcs_flag == 2:
        if pos + 4 > len(src):
            return None
        return int.from_bytes(src[pos:pos + 4], 'little')
    if pos + 8 > len(src):
        return None
    return int.from_bytes(src[pos:pos + 8], 'little')


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
