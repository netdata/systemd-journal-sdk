# Journal file verification.
# Validates structural integrity of unsealed journal files.
# Sealed FSS tag/HMAC verification is implemented for sealed files with a key.

import hmac as hmac_mod
import hashlib
import os

from .reader import FileReader
from .entry import parse_entry_object, parse_data_object
from .header import (
    INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_COMPRESSED_ZSTD, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS,
    HEADER_MIN_SIZE, OBJECT_HEADER_SIZE, OBJECT_TYPE_DATA,
    OBJECT_TYPE_FIELD, OBJECT_TYPE_ENTRY, OBJECT_TYPE_DATA_HASH_TABLE,
    OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY,
    DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE, FIELD_OBJECT_HEADER_SIZE,
    OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
    parse_file_header,
)
from .compress import is_zst_file, decompress_zst_to_temp
from .fss import gen_mk, gen_state0, seek, get_key, RECOMMENDED_SECPAR
from .seal import TAG_LENGTH, OBJECT_TYPE_TAG
from .verify_graph import ObjectGraphVerificationError, verify_object_graph

MAX_U64 = (1 << 64) - 1


class VerificationError(Exception):
    """Raised when a journal file fails structural integrity verification."""


def verify_file(path, verify_key=None):
    """Validate the structural integrity of a journal file.

    Opens the file (decompressing .zst if needed), validates the header,
    and walks all entries and their referenced data objects strictly.
    Any parse or decompression error is reported as a VerificationError.

    For sealed journals with a verify_key, validates TAG/HMAC chains.
    For sealed journals without a key, verifies structure only; callers that
    require cryptographic verification should call verify_file_with_key().
    """
    if verify_key is not None:
        return verify_file_with_key(path, verify_key)

    try:
        verify_object_graph(_read_journal_file_bytes(path))
    except ObjectGraphVerificationError as err:
        raise VerificationError(f'journal verification failed: corrupt object graph: {err}') from err
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt or unreadable file: {err}"
        ) from err

    r = None
    try:
        r = FileReader.open(path)
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt or unreadable file: {err}"
        ) from err

    try:
        # Verification walks internal parser state so corrupt data objects fail
        # instead of being skipped by the normal reader tolerance path.
        buf = r._buffer
        compact = (r._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
        entry_monotonic = 0
        entry_monotonic_set = False
        entry_boot_id = b'\x00' * 16

        for offset in r._entry_offsets:
            # Parse entry object strictly
            try:
                e = parse_entry_object(buf, offset, compact)
            except Exception as err:
                raise VerificationError(
                    f"journal verification failed: corrupt entry object at offset {offset}: {err}"
                ) from err

            if (
                entry_monotonic_set and
                e['boot_id'] == entry_boot_id and
                entry_monotonic > e['monotonic']
            ):
                raise VerificationError(
                    f"journal verification failed: entry monotonic out of sync "
                    f"({entry_monotonic} > {e['monotonic']})"
                )
            entry_monotonic = e['monotonic']
            entry_boot_id = e['boot_id']
            entry_monotonic_set = True

            # Parse each referenced data object strictly
            for item in e['items']:
                data_off = item['offset']
                try:
                    parse_data_object(buf, data_off, compact)
                except Exception as err:
                    raise VerificationError(
                        f"journal verification failed: corrupt data object at offset {data_off} "
                        f"for entry at offset {offset}: {err}"
                    ) from err
    finally:
        r.close()


def verify_file_with_key(path, verification_key):
    """Validate the integrity of a journal file with a verification key.

    For sealed files, parses the key and validates TAG/HMAC chains.
    For unsealed files, behaves like verify_file.
    """
    try:
        data = _read_journal_file_bytes(path)
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt or unreadable file: {err}"
        ) from err

    if len(data) < HEADER_MIN_SIZE:
        raise VerificationError('journal verification failed: file too small')

    try:
        verify_object_graph(data)
    except ObjectGraphVerificationError as err:
        raise VerificationError(f'journal verification failed: corrupt object graph: {err}') from err

    try:
        header = parse_file_header(data)
    except Exception as err:
        raise VerificationError(f'journal verification failed: invalid header: {err}') from err
    sealed = (header['compatible_flags'] & COMPATIBLE_SEALED) != 0

    if not sealed:
        return verify_file(path)

    seed, start_epoch, interval_usec = _parse_verification_key(verification_key)
    _verify_sealed(data, header, seed, start_epoch, interval_usec)
    return verify_file(path)


def _read_journal_file_bytes(path):
    cleanup_path = None
    try:
        if is_zst_file(path):
            cleanup_path = decompress_zst_to_temp(path, 'python-sdk-verify')
            with open(cleanup_path, 'rb') as f:
                return f.read()
        with open(path, 'rb') as f:
            return f.read()
    finally:
        if cleanup_path:
            try:
                os.unlink(cleanup_path)
                os.rmdir(os.path.dirname(cleanup_path))
            except OSError:
                pass


def _parse_verification_key(key):
    if not isinstance(key, str):
        raise VerificationError('invalid verification key: not a string')
    seed = bytearray(12)
    i = 0
    for c in range(12):
        while i < len(key) and key[i] == '-':
            i += 1
        if i + 2 > len(key):
            raise VerificationError('invalid verification key: seed too short')
        try:
            b = int(key[i:i + 2], 16)
        except ValueError:
            raise VerificationError('invalid verification key: bad seed hex')
        seed[c] = b
        i += 2
    if i >= len(key) or key[i] != '/':
        raise VerificationError('invalid verification key: missing / separator')
    i += 1

    next_i, ok = _consume_hex(key, i)
    if not ok or next_i >= len(key) or key[next_i] != '-':
        raise VerificationError('invalid verification key: bad start hex')
    try:
        start_epoch = int(key[i:next_i], 16)
    except ValueError:
        raise VerificationError('invalid verification key: bad start hex')
    if start_epoch > MAX_U64:
        raise VerificationError('invalid verification key: bad start hex')

    i = next_i + 1
    next_i, ok = _consume_hex(key, i)
    if not ok:
        raise VerificationError('invalid verification key: bad interval hex')
    try:
        interval_usec = int(key[i:next_i], 16)
    except ValueError:
        raise VerificationError('invalid verification key: bad interval hex')
    if next_i != len(key):
        raise VerificationError('invalid verification key: trailing data')
    if interval_usec == 0:
        raise VerificationError('invalid verification key: zero interval')
    if interval_usec > MAX_U64:
        raise VerificationError('invalid verification key: bad interval hex')

    return bytes(seed), start_epoch, interval_usec


def _consume_hex(s, start):
    i = start
    while i < len(s) and _is_hex(s[i]):
        i += 1
    return i, i > start


def _is_hex(ch):
    return ch in '0123456789abcdefABCDEF'


def _align8(v):
    return (v + 7) & ~7


def _tag_realtime_range(start_epoch, epoch, interval_usec):
    absolute_epoch = start_epoch + epoch
    if absolute_epoch > MAX_U64:
        raise VerificationError('tag realtime overflow')
    rt = absolute_epoch * interval_usec
    if rt > MAX_U64:
        raise VerificationError('tag realtime overflow')
    rt_end = rt + interval_usec
    if rt_end > MAX_U64:
        raise VerificationError('tag realtime overflow')
    return rt, rt_end


def _verify_sealed(data, header, seed, start_epoch, interval_usec):
    is_compact = (header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0

    msk, mpk = gen_mk(seed, RECOMMENDED_SECPAR)
    state0 = gen_state0(mpk, seed)

    header_size = header['header_size']
    tail_object_offset = header['tail_object_offset']
    file_size = len(data)
    if header_size < HEADER_MIN_SIZE or header_size > file_size:
        raise VerificationError(f'invalid header_size {header_size}')

    n_objects = 0
    n_entries = 0
    n_tags = 0
    last_tag_end = 0
    last_epoch = 0
    last_tag_realtime = 0
    entry_seqnum = 0
    entry_seqnum_set = False
    entry_monotonic = 0
    entry_monotonic_set = False
    entry_boot_id = b'\x00' * 16
    entry_realtime = 0
    entry_realtime_set = False
    max_entry_realtime = 0
    min_entry_realtime = (1 << 64) - 1

    p = header_size
    while True:
        if tail_object_offset == 0:
            break
        if p > tail_object_offset:
            raise VerificationError(
                f'object offset {p} exceeds tail_object_offset {tail_object_offset}')
        if p + OBJECT_HEADER_SIZE > file_size:
            raise VerificationError(
                f'object header at offset {p} exceeds file bounds')

        typ = data[p]
        flags = data[p + 1]
        size = int.from_bytes(data[p + 8:p + 16], 'little')
        aligned_size = _align8(size)

        if size < OBJECT_HEADER_SIZE:
            raise VerificationError(
                f'object size {size} too small at offset {p}')
        if p + aligned_size > file_size:
            raise VerificationError(
                f'object at offset {p} with aligned size {aligned_size} exceeds file bounds')

        compression_flags = 0
        if flags & OBJECT_COMPRESSED_XZ:
            compression_flags += 1
        if flags & OBJECT_COMPRESSED_LZ4:
            compression_flags += 1
        if flags & OBJECT_COMPRESSED_ZSTD:
            compression_flags += 1
        if compression_flags > 1:
            raise VerificationError(f'multiple compression flags at offset {p}')
        if (flags & OBJECT_COMPRESSED_XZ) and not (header['incompatible_flags'] & INCOMPATIBLE_COMPRESSED_XZ):
            raise VerificationError(f'XZ object in file without XZ support at offset {p}')
        if (flags & OBJECT_COMPRESSED_LZ4) and not (header['incompatible_flags'] & INCOMPATIBLE_COMPRESSED_LZ4):
            raise VerificationError(f'LZ4 object in file without LZ4 support at offset {p}')
        if (flags & OBJECT_COMPRESSED_ZSTD) and not (header['incompatible_flags'] & INCOMPATIBLE_COMPRESSED_ZSTD):
            raise VerificationError(f'ZSTD object in file without ZSTD support at offset {p}')
        if flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD):
            raise VerificationError(f'unknown object flags 0x{flags:x} at offset {p}')
        if typ != OBJECT_TYPE_DATA and flags:
            raise VerificationError(f'object type {typ} at offset {p} has compression flags')

        n_objects += 1

        if typ == OBJECT_TYPE_DATA:
            pass
        elif typ == OBJECT_TYPE_FIELD:
            pass
        elif typ == OBJECT_TYPE_ENTRY:
            if n_tags == 0:
                raise VerificationError(f'first entry before first tag at offset {p}')
            e_seqnum = int.from_bytes(data[p + 16:p + 24], 'little')
            e_realtime = int.from_bytes(data[p + 24:p + 32], 'little')
            e_monotonic = int.from_bytes(data[p + 32:p + 40], 'little')
            e_boot_id = data[p + 40:p + 56]

            if entry_realtime_set and e_realtime < last_tag_realtime:
                raise VerificationError(f'older entry after newer tag at offset {p}')
            if not entry_seqnum_set:
                if e_seqnum != header['head_entry_seqnum']:
                    raise VerificationError(f'head entry seqnum mismatch at offset {p}')
            else:
                if entry_seqnum >= e_seqnum:
                    raise VerificationError(f'entry seqnum out of sync at offset {p}')
            entry_seqnum = e_seqnum
            entry_seqnum_set = True

            if entry_monotonic_set and e_boot_id == entry_boot_id and entry_monotonic > e_monotonic:
                raise VerificationError(f'entry monotonic out of sync at offset {p}')
            entry_monotonic = e_monotonic
            entry_boot_id = e_boot_id
            entry_monotonic_set = True

            if not entry_realtime_set:
                if e_realtime != header['head_entry_realtime']:
                    raise VerificationError(f'head entry realtime mismatch at offset {p}')
            entry_realtime = e_realtime
            entry_realtime_set = True

            if e_realtime > max_entry_realtime:
                max_entry_realtime = e_realtime
            if e_realtime < min_entry_realtime:
                min_entry_realtime = e_realtime

            n_entries += 1
        elif typ == OBJECT_TYPE_DATA_HASH_TABLE:
            pass
        elif typ == OBJECT_TYPE_FIELD_HASH_TABLE:
            pass
        elif typ == OBJECT_TYPE_ENTRY_ARRAY:
            pass
        elif typ == OBJECT_TYPE_TAG:
            if size != OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH:
                raise VerificationError(
                    f'invalid tag object size {size} at offset {p}')
            seqnum = int.from_bytes(data[p + 16:p + 24], 'little')
            epoch = int.from_bytes(data[p + 24:p + 32], 'little')

            if seqnum != n_tags + 1:
                raise VerificationError(
                    f'tag seqnum mismatch: got {seqnum}, want {n_tags + 1} at offset {p}')

            sealed_continuous = (header['compatible_flags'] & COMPATIBLE_SEALED_CONTINUOUS) != 0
            if sealed_continuous:
                ok = (n_tags == 0 or
                      (n_tags == 1 and epoch == last_epoch) or
                      epoch == last_epoch + 1)
                if not ok:
                    raise VerificationError(
                        f'epoch not continuous: got {epoch}, last {last_epoch} at offset {p}')
            else:
                if epoch < last_epoch:
                    raise VerificationError(
                        f'epoch out of sync: got {epoch}, last {last_epoch} at offset {p}')

            rt, rt_end = _tag_realtime_range(start_epoch, epoch, interval_usec)

            if entry_realtime_set and entry_realtime >= rt_end:
                raise VerificationError(
                    f'entry realtime {entry_realtime} too late for tag end {rt_end} at offset {p}')
            if max_entry_realtime >= rt_end:
                raise VerificationError(
                    f'max entry realtime {max_entry_realtime} too late for tag end {rt_end} at offset {p}')
            if min_entry_realtime < rt:
                raise VerificationError(
                    f'entry realtime {min_entry_realtime} too early for tag start {rt} at offset {p}')

            # Compute HMAC
            state = seek(state0, epoch, msk, seed)
            key = get_key(state, TAG_LENGTH, 0)
            hm = hmac_mod.new(key, digestmod=hashlib.sha256)

            if n_tags == 0:
                hm.update(data[0:16])
                hm.update(data[24:56])
                hm.update(data[72:96])
                hm.update(data[104:136])

            q = last_tag_end
            if n_tags == 0:
                q = header_size

            while q <= p:
                if q + OBJECT_HEADER_SIZE > file_size:
                    raise VerificationError(
                        f'HMAC object header at offset {q} exceeds file bounds')
                q_typ = data[q]
                q_size = int.from_bytes(data[q + 8:q + 16], 'little')
                if q_size < OBJECT_HEADER_SIZE:
                    raise VerificationError(
                        f'HMAC object size {q_size} too small at offset {q}')
                q_aligned_size = _align8(q_size)
                if q_aligned_size < q_size or q_aligned_size == 0:
                    raise VerificationError(
                        f'HMAC object size {q_size} overflows alignment at offset {q}')
                if q_aligned_size > file_size - q:
                    raise VerificationError(
                        f'HMAC object at offset {q} with aligned size {q_aligned_size} exceeds file bounds')
                _hmac_object(hm, data, q, q_typ, q_size, is_compact)
                q += q_aligned_size

            computed = hm.digest()
            stored = data[p + 32:p + 32 + TAG_LENGTH]
            if not hmac_mod.compare_digest(computed, stored):
                raise VerificationError(f'tag failed verification at offset {p}')

            n_tags += 1
            last_tag_end = p + aligned_size
            last_epoch = epoch
            last_tag_realtime = rt
            min_entry_realtime = (1 << 64) - 1
        else:
            raise VerificationError(f'unknown object type {typ} at offset {p}')

        if p == tail_object_offset:
            break
        p += aligned_size

    if n_objects != header['n_objects']:
        raise VerificationError(
            f'object count mismatch: got {n_objects}, want {header["n_objects"]}')
    if n_entries != header['n_entries']:
        raise VerificationError(
            f'entry count mismatch: got {n_entries}, want {header["n_entries"]}')
    if n_tags != header['n_tags']:
        raise VerificationError(
            f'tag count mismatch: got {n_tags}, want {header["n_tags"]}')


def _hmac_object(hm, data, offset, typ, size, is_compact):
    hm.update(data[offset:offset + OBJECT_HEADER_SIZE])

    if typ == OBJECT_TYPE_DATA:
        hm.update(data[offset + 16:offset + 24])
        payload_offset = DATA_OBJECT_HEADER_SIZE
        if is_compact:
            payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE
        if size > payload_offset:
            hm.update(data[offset + payload_offset:offset + size])
    elif typ == OBJECT_TYPE_FIELD:
        hm.update(data[offset + 16:offset + 24])
        if size > FIELD_OBJECT_HEADER_SIZE:
            hm.update(data[offset + FIELD_OBJECT_HEADER_SIZE:offset + size])
    elif typ == OBJECT_TYPE_ENTRY:
        if size > OBJECT_HEADER_SIZE:
            hm.update(data[offset + OBJECT_HEADER_SIZE:offset + size])
    elif typ in (OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY):
        pass
    elif typ == OBJECT_TYPE_TAG:
        hm.update(data[offset + OBJECT_HEADER_SIZE:offset + OBJECT_HEADER_SIZE + 16])
