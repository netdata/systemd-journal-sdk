import importlib.util
import lzma
import struct

COMPRESSION_NONE = 0
COMPRESSION_ZSTD = 1
COMPRESSION_XZ = 2
COMPRESSION_LZ4 = 3
DEFAULT_COMPRESS_THRESHOLD = 512
MIN_COMPRESS_THRESHOLD = 8


def _compressed_payload(payload, compressor, compression_flag):
    compressed = _best_effort_compress(compressor, payload)
    if compressed is not None and len(compressed) < len(payload):
        return compressed, compression_flag
    return payload, 0


def _normalize_compression(value):
    if value is None or value == COMPRESSION_NONE or value == 'none':
        return COMPRESSION_NONE
    if value == COMPRESSION_ZSTD or value == 'zstd':
        return COMPRESSION_ZSTD
    if value == COMPRESSION_XZ or value == 'xz':
        return COMPRESSION_XZ
    if value == COMPRESSION_LZ4 or value == 'lz4':
        return COMPRESSION_LZ4
    raise ValueError(f'unsupported compression: {value}')


def _normalize_compress_threshold(value):
    if value is None:
        return DEFAULT_COMPRESS_THRESHOLD
    threshold = int(value)
    return max(MIN_COMPRESS_THRESHOLD, threshold)


def _zstd_compress(payload):
    import compression.zstd
    return compression.zstd.compress(payload)


def _xz_compress(payload):
    return lzma.compress(
        payload,
        format=lzma.FORMAT_XZ,
        check=lzma.CHECK_NONE,
        filters=[{'id': lzma.FILTER_LZMA2, 'preset': 0}],
    )


def _lz4_compress(payload):
    import lz4.block
    compressed = lz4.block.compress(payload, store_size=False)
    size_prefix = struct.pack('<Q', len(payload))
    return size_prefix + compressed


def _best_effort_compress(compressor, payload):
    try:
        return compressor(payload)
    except Exception:
        return None


def _ensure_zstd_available():
    if not _module_available('compression.zstd'):
        raise ImportError('compression.zstd is required for zstd journal compression')


def _ensure_xz_available():
    if not _module_available('lzma'):
        raise ImportError('lzma is required for xz journal compression')


def _ensure_lz4_available():
    if not _module_available('lz4.block'):
        raise ImportError('lz4.block is required for lz4 journal compression')


def _module_available(name):
    try:
        return importlib.util.find_spec(name) is not None
    except ModuleNotFoundError:
        return False
