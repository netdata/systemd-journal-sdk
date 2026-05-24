# Low-level binary helpers for systemd journal files.
# All 64-bit values use Python int (unlimited precision).

import struct
import os


def read_uint64_le(buf, offset=0):
    return struct.unpack('<Q', buf[offset:offset + 8])[0]


def write_uint64_le(buf, offset, value):
    struct.pack_into('<Q', buf, offset, value)


def write_uint32_le(buf, offset, value):
    struct.pack_into('<I', buf, offset, value)


def write_uint8(buf, offset, value):
    struct.pack_into('<B', buf, offset, value)


def align8(value):
    return (value + 7) & ~7


def buf_equal(a, b):
    return a == b


def uuid_to_string(uuid):
    return uuid.hex()


def string_to_uuid(hex_str):
    return bytes.fromhex(hex_str)


def is_zero_uuid(uuid):
    for b in uuid:
        if b != 0:
            return False
    return True


def random_uuid():
    return os.urandom(16)
