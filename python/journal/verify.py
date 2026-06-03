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
        verify_file_with_key(path, verify_key)
        return None

    _verify_object_graph_bytes(_read_journal_file_bytes(path))
    _verify_reader_entry_payloads(path)
    return None


def _verify_object_graph_bytes(data):
    try:
        verify_object_graph(data)
    except ObjectGraphVerificationError as err:
        raise VerificationError(f'journal verification failed: corrupt object graph: {err}') from err
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt or unreadable file: {err}"
        ) from err


def _verify_reader_entry_payloads(path):
    try:
        with FileReader.open(path) as reader:
            _verify_reader_entry_offsets(reader)
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt or unreadable file: {err}"
        ) from err


def _verify_reader_entry_offsets(reader):
    # Verification walks internal parser state so corrupt data objects fail
    # instead of being skipped by the normal reader tolerance path.
    buf = reader._buffer
    compact = (reader._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
    monotonic_state = {'set': False, 'value': 0, 'boot_id': b'\x00' * 16}
    for offset in reader._entry_offsets:
        entry = _parse_strict_entry(buf, offset, compact)
        _validate_entry_monotonic(offset, entry, monotonic_state)
        _parse_strict_entry_data(buf, offset, entry, compact)


def _parse_strict_entry(buf, offset, compact):
    try:
        return parse_entry_object(buf, offset, compact)
    except Exception as err:
        raise VerificationError(
            f"journal verification failed: corrupt entry object at offset {offset}: {err}"
        ) from err


def _validate_entry_monotonic(offset, entry, state):
    if state['set'] and entry['boot_id'] == state['boot_id'] and state['value'] > entry['monotonic']:
        raise VerificationError(
            f"journal verification failed: entry monotonic out of sync "
            f"({state['value']} > {entry['monotonic']})"
        )
    state['value'] = entry['monotonic']
    state['boot_id'] = entry['boot_id']
    state['set'] = True


def _parse_strict_entry_data(buf, entry_offset, entry, compact):
    for item in entry['items']:
        data_off = item['offset']
        try:
            parse_data_object(buf, data_off, compact)
        except Exception as err:
            raise VerificationError(
                f"journal verification failed: corrupt data object at offset {data_off} "
                f"for entry at offset {entry_offset}: {err}"
            ) from err


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

    seed, i = _parse_key_seed(key)
    if i >= len(key) or key[i] != '/':
        raise VerificationError('invalid verification key: missing / separator')
    i += 1

    start_epoch, i = _parse_hex_component(key, i, 'start', separator='-')
    interval_usec, next_i = _parse_hex_component(key, i, 'interval')
    if next_i != len(key):
        raise VerificationError('invalid verification key: trailing data')
    if interval_usec == 0:
        raise VerificationError('invalid verification key: zero interval')

    return bytes(seed), start_epoch, interval_usec


def _parse_key_seed(key):
    seed = bytearray(12)
    i = 0
    for c in range(12):
        i = _skip_key_dashes(key, i)
        if i + 2 > len(key):
            raise VerificationError('invalid verification key: seed too short')
        seed[c] = _parse_seed_byte(key[i:i + 2])
        i += 2
    return seed, i


def _skip_key_dashes(key, index):
    while index < len(key) and key[index] == '-':
        index += 1
    return index


def _parse_seed_byte(value):
    try:
        return int(value, 16)
    except ValueError:
        raise VerificationError('invalid verification key: bad seed hex')


def _parse_hex_component(key, index, label, separator=None):
    next_i, ok = _consume_hex(key, index)
    if not ok:
        raise VerificationError(f'invalid verification key: bad {label} hex')
    if separator is not None and (next_i >= len(key) or key[next_i] != separator):
        raise VerificationError(f'invalid verification key: bad {label} hex')
    try:
        value = int(key[index:next_i], 16)
    except ValueError:
        raise VerificationError(f'invalid verification key: bad {label} hex')
    if value > MAX_U64:
        raise VerificationError(f'invalid verification key: bad {label} hex')
    if separator is not None:
        next_i += 1
    return value, next_i


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
    _SealedVerifier(data, header, seed, start_epoch, interval_usec).verify()


class _SealedVerifier:
    def __init__(self, data, header, seed, start_epoch, interval_usec):
        self.data = data
        self.header = header
        self.seed = seed
        self.start_epoch = start_epoch
        self.interval_usec = interval_usec
        self.is_compact = (header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
        self.msk, mpk = gen_mk(seed, RECOMMENDED_SECPAR)
        self.state0 = gen_state0(mpk, seed)
        self.n_objects = 0
        self.n_entries = 0
        self.n_tags = 0
        self.last_tag_end = 0
        self.last_epoch = 0
        self.last_tag_realtime = 0
        self.entry_seqnum = 0
        self.entry_seqnum_set = False
        self.entry_monotonic = 0
        self.entry_monotonic_set = False
        self.entry_boot_id = b'\x00' * 16
        self.entry_realtime = 0
        self.entry_realtime_set = False
        self.max_entry_realtime = 0
        self.min_entry_realtime = MAX_U64

    def verify(self):
        self._validate_header_size()
        self._walk_objects()
        self._validate_counts()

    def _validate_header_size(self):
        header_size = self.header['header_size']
        if header_size < HEADER_MIN_SIZE or header_size > len(self.data):
            raise VerificationError(f'invalid header_size {header_size}')

    def _walk_objects(self):
        offset = self.header['header_size']
        while True:
            if self.header['tail_object_offset'] == 0:
                break
            obj = self._read_object(offset)
            self.n_objects += 1
            self._handle_object(obj)
            if offset == self.header['tail_object_offset']:
                break
            offset += obj['aligned_size']

    def _read_object(self, offset):
        self._validate_object_bounds(offset)
        typ = self.data[offset]
        flags = self.data[offset + 1]
        size = int.from_bytes(self.data[offset + 8:offset + 16], 'little')
        aligned_size = _align8(size)
        self._validate_object_size(offset, size, aligned_size)
        self._validate_object_flags(offset, typ, flags)
        return {'offset': offset, 'type': typ, 'flags': flags, 'size': size, 'aligned_size': aligned_size}

    def _validate_object_bounds(self, offset):
        tail_object_offset = self.header['tail_object_offset']
        if offset > tail_object_offset:
            raise VerificationError(f'object offset {offset} exceeds tail_object_offset {tail_object_offset}')
        if offset + OBJECT_HEADER_SIZE > len(self.data):
            raise VerificationError(f'object header at offset {offset} exceeds file bounds')

    def _validate_object_size(self, offset, size, aligned_size):
        if size < OBJECT_HEADER_SIZE:
            raise VerificationError(f'object size {size} too small at offset {offset}')
        if offset + aligned_size > len(self.data):
            raise VerificationError(
                f'object at offset {offset} with aligned size {aligned_size} exceeds file bounds'
            )

    def _validate_object_flags(self, offset, typ, flags):
        if _compression_flag_count(flags) > 1:
            raise VerificationError(f'multiple compression flags at offset {offset}')
        if flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD):
            raise VerificationError(f'unknown object flags 0x{flags:x} at offset {offset}')
        if typ != OBJECT_TYPE_DATA and flags:
            raise VerificationError(f'object type {typ} at offset {offset} has compression flags')
        self._validate_compression_header_flag(offset, flags, OBJECT_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_XZ, 'XZ')
        self._validate_compression_header_flag(offset, flags, OBJECT_COMPRESSED_LZ4, INCOMPATIBLE_COMPRESSED_LZ4, 'LZ4')
        self._validate_compression_header_flag(offset, flags, OBJECT_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_ZSTD, 'ZSTD')

    def _validate_compression_header_flag(self, offset, flags, object_flag, header_flag, name):
        if (flags & object_flag) and not (self.header['incompatible_flags'] & header_flag):
            raise VerificationError(f'{name} object in file without {name} support at offset {offset}')

    def _handle_object(self, obj):
        typ = obj['type']
        if typ in (OBJECT_TYPE_DATA, OBJECT_TYPE_FIELD, OBJECT_TYPE_DATA_HASH_TABLE,
                   OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY):
            return
        if typ == OBJECT_TYPE_ENTRY:
            self._handle_entry(obj['offset'])
            return
        if typ == OBJECT_TYPE_TAG:
            self._handle_tag(obj)
            return
        raise VerificationError(f'unknown object type {typ} at offset {obj["offset"]}')

    def _handle_entry(self, offset):
        if self.n_tags == 0:
            raise VerificationError(f'first entry before first tag at offset {offset}')
        entry = self._read_entry(offset)
        self._validate_entry_order(offset, entry)
        self._update_entry_state(entry)
        self.n_entries += 1

    def _read_entry(self, offset):
        return {
            'seqnum': int.from_bytes(self.data[offset + 16:offset + 24], 'little'),
            'realtime': int.from_bytes(self.data[offset + 24:offset + 32], 'little'),
            'monotonic': int.from_bytes(self.data[offset + 32:offset + 40], 'little'),
            'boot_id': self.data[offset + 40:offset + 56],
        }

    def _validate_entry_order(self, offset, entry):
        self._validate_entry_realtime_order(offset, entry['realtime'])
        self._validate_entry_seqnum_order(offset, entry['seqnum'])
        self._validate_entry_monotonic_order(offset, entry)

    def _validate_entry_realtime_order(self, offset, realtime):
        if self.entry_realtime_set and realtime < self.last_tag_realtime:
            raise VerificationError(f'older entry after newer tag at offset {offset}')
        if not self.entry_realtime_set and realtime != self.header['head_entry_realtime']:
            raise VerificationError(f'head entry realtime mismatch at offset {offset}')

    def _validate_entry_seqnum_order(self, offset, seqnum):
        if not self.entry_seqnum_set and seqnum != self.header['head_entry_seqnum']:
            raise VerificationError(f'head entry seqnum mismatch at offset {offset}')
        if self.entry_seqnum_set and self.entry_seqnum >= seqnum:
            raise VerificationError(f'entry seqnum out of sync at offset {offset}')

    def _validate_entry_monotonic_order(self, offset, entry):
        if (
            self.entry_monotonic_set
            and entry['boot_id'] == self.entry_boot_id
            and self.entry_monotonic > entry['monotonic']
        ):
            raise VerificationError(f'entry monotonic out of sync at offset {offset}')

    def _update_entry_state(self, entry):
        self.entry_seqnum = entry['seqnum']
        self.entry_seqnum_set = True
        self.entry_monotonic = entry['monotonic']
        self.entry_boot_id = entry['boot_id']
        self.entry_monotonic_set = True
        self.entry_realtime = entry['realtime']
        self.entry_realtime_set = True
        self.max_entry_realtime = max(self.max_entry_realtime, entry['realtime'])
        self.min_entry_realtime = min(self.min_entry_realtime, entry['realtime'])

    def _handle_tag(self, obj):
        offset = obj['offset']
        if obj['size'] != OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH:
            raise VerificationError(f'invalid tag object size {obj["size"]} at offset {offset}')
        seqnum = int.from_bytes(self.data[offset + 16:offset + 24], 'little')
        epoch = int.from_bytes(self.data[offset + 24:offset + 32], 'little')
        if seqnum != self.n_tags + 1:
            raise VerificationError(f'tag seqnum mismatch: got {seqnum}, want {self.n_tags + 1} at offset {offset}')
        self._validate_tag_epoch(offset, epoch)
        rt, rt_end = _tag_realtime_range(self.start_epoch, epoch, self.interval_usec)
        self._validate_tag_realtime(offset, rt, rt_end)
        self._verify_tag_hmac(offset, epoch)
        self._update_tag_state(obj, epoch, rt)

    def _validate_tag_epoch(self, offset, epoch):
        sealed_continuous = (self.header['compatible_flags'] & COMPATIBLE_SEALED_CONTINUOUS) != 0
        if sealed_continuous:
            ok = self.n_tags == 0 or (self.n_tags == 1 and epoch == self.last_epoch) or epoch == self.last_epoch + 1
            if not ok:
                raise VerificationError(f'epoch not continuous: got {epoch}, last {self.last_epoch} at offset {offset}')
        elif epoch < self.last_epoch:
            raise VerificationError(f'epoch out of sync: got {epoch}, last {self.last_epoch} at offset {offset}')

    def _validate_tag_realtime(self, offset, rt, rt_end):
        if self.entry_realtime_set and self.entry_realtime >= rt_end:
            raise VerificationError(
                f'entry realtime {self.entry_realtime} too late for tag end {rt_end} at offset {offset}'
            )
        if self.max_entry_realtime >= rt_end:
            raise VerificationError(
                f'max entry realtime {self.max_entry_realtime} too late for tag end {rt_end} at offset {offset}'
            )
        if self.min_entry_realtime < rt:
            raise VerificationError(
                f'entry realtime {self.min_entry_realtime} too early for tag start {rt} at offset {offset}'
            )

    def _verify_tag_hmac(self, offset, epoch):
        hm = self._new_tag_hmac(epoch)
        q = self.header['header_size'] if self.n_tags == 0 else self.last_tag_end
        while q <= offset:
            q_typ, q_size, q_aligned_size = self._read_hmac_object(q)
            _hmac_object(hm, self.data, q, q_typ, q_size, self.is_compact)
            q += q_aligned_size
        stored = self.data[offset + 32:offset + 32 + TAG_LENGTH]
        if not hmac_mod.compare_digest(hm.digest(), stored):
            raise VerificationError(f'tag failed verification at offset {offset}')

    def _new_tag_hmac(self, epoch):
        state = seek(self.state0, epoch, self.msk, self.seed)
        key = get_key(state, TAG_LENGTH, 0)
        hm = hmac_mod.new(key, digestmod=hashlib.sha256)
        if self.n_tags == 0:
            hm.update(self.data[0:16])
            hm.update(self.data[24:56])
            hm.update(self.data[72:96])
            hm.update(self.data[104:136])
        return hm

    def _read_hmac_object(self, offset):
        if offset + OBJECT_HEADER_SIZE > len(self.data):
            raise VerificationError(f'HMAC object header at offset {offset} exceeds file bounds')
        typ = self.data[offset]
        size = int.from_bytes(self.data[offset + 8:offset + 16], 'little')
        if size < OBJECT_HEADER_SIZE:
            raise VerificationError(f'HMAC object size {size} too small at offset {offset}')
        aligned_size = _align8(size)
        if aligned_size < size or aligned_size == 0:
            raise VerificationError(f'HMAC object size {size} overflows alignment at offset {offset}')
        if aligned_size > len(self.data) - offset:
            raise VerificationError(
                f'HMAC object at offset {offset} with aligned size {aligned_size} exceeds file bounds'
            )
        return typ, size, aligned_size

    def _update_tag_state(self, obj, epoch, rt):
        self.n_tags += 1
        self.last_tag_end = obj['offset'] + obj['aligned_size']
        self.last_epoch = epoch
        self.last_tag_realtime = rt
        self.min_entry_realtime = MAX_U64

    def _validate_counts(self):
        if self.n_objects != self.header['n_objects']:
            raise VerificationError(f'object count mismatch: got {self.n_objects}, want {self.header["n_objects"]}')
        if self.n_entries != self.header['n_entries']:
            raise VerificationError(f'entry count mismatch: got {self.n_entries}, want {self.header["n_entries"]}')
        if self.n_tags != self.header['n_tags']:
            raise VerificationError(f'tag count mismatch: got {self.n_tags}, want {self.header["n_tags"]}')


def _compression_flag_count(flags):
    count = 0
    for object_flag in (OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD):
        if flags & object_flag:
            count += 1
    return count


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
