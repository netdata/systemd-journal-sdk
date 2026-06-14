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
        raise TypeError('compressed DATA payload must be bytes-like')
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
    raise TypeError('compressed DATA payload must be bytes-like')


def _zstd_frame_content_size(src):
    header = _zstd_frame_header(src)
    if header is None:
        return None
    pos = _zstd_frame_content_size_offset(src, header)
    if pos is None:
        return None
    return _read_zstd_frame_content_size(src, pos, header['fcs_flag'], header['single_segment'])


def _zstd_frame_header(src):
    if len(src) < 6 or src[:4] != b'\x28\xb5\x2f\xfd':
        return None
    descriptor = src[4]
    return {
        'fcs_flag': descriptor >> 6,
        'single_segment': bool(descriptor & 0x20),
        'dictionary_id_flag': descriptor & 0x03,
    }


def _zstd_frame_content_size_offset(src, header):
    pos = 5 + (0 if header['single_segment'] else 1)
    if pos > len(src):
        return None
    dict_id_sizes = (0, 1, 2, 4)
    pos += dict_id_sizes[header['dictionary_id_flag']]
    if pos > len(src):
        return None
    return pos


def _read_zstd_frame_content_size(src, pos, fcs_flag, single_segment):
    size_len = _zstd_frame_content_size_len(fcs_flag, single_segment)
    if size_len == 0 or pos + size_len > len(src):
        return None
    value = int.from_bytes(src[pos:pos + size_len], 'little')
    return value + 256 if fcs_flag == 1 else value


def _zstd_frame_content_size_len(fcs_flag, single_segment):
    if fcs_flag == 0:
        return 1 if single_segment else 0
    if fcs_flag == 1:
        return 2
    if fcs_flag == 2:
        return 4
    return 8


def stream_zst_to_temp(input_path, prefix='python-journal', chunk_size=1024 * 1024):
    if not _HAS_ZSTD:
        raise RuntimeError('zstd decompression not available')
    chunk_size = int(chunk_size)
    if chunk_size <= 0:
        raise ValueError('chunk_size must be positive')
    temp_dir = tempfile.mkdtemp(prefix=prefix + '-')
    try:
        temp_path = os.path.join(temp_dir, 'decompressed.journal')
        with compression.zstd.open(input_path, 'rb') as src, open(temp_path, 'wb') as dst:
            while True:
                chunk = src.read(chunk_size)
                if not chunk:
                    break
                dst.write(chunk)
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
