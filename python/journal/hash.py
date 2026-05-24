# SipHash-2-4 and Jenkins hash for systemd journal files.
# SipHash returns Python int for full 64-bit precision.
# Jenkins lookup3 returns Python int for 64-bit.


def _rotl64(v, n):
    return ((v << n) | (v >> (64 - n))) & 0xFFFFFFFFFFFFFFFF


def sip_hash_24(key, msg):
    if isinstance(msg, str):
        msg = msg.encode('latin1')
    elif not isinstance(msg, (bytes, bytearray, memoryview)):
        msg = bytes(msg)

    k0 = int.from_bytes(key[0:8], 'little')
    k1 = int.from_bytes(key[8:16], 'little')

    v0 = 0x736F6D6570736575 ^ k0
    v1 = 0x646F72616E646F6D ^ k1
    v2 = 0x6C7967656E657261 ^ k0
    v3 = 0x7465646279746573 ^ k1

    def _round():
        nonlocal v0, v1, v2, v3
        v0 = (v0 + v1) & 0xFFFFFFFFFFFFFFFF
        v1 = _rotl64(v1, 13) & 0xFFFFFFFFFFFFFFFF
        v1 ^= v0
        v0 = _rotl64(v0, 32) & 0xFFFFFFFFFFFFFFFF
        v2 = (v2 + v3) & 0xFFFFFFFFFFFFFFFF
        v3 = _rotl64(v3, 16) & 0xFFFFFFFFFFFFFFFF
        v3 ^= v2
        v0 = (v0 + v3) & 0xFFFFFFFFFFFFFFFF
        v3 = _rotl64(v3, 21) & 0xFFFFFFFFFFFFFFFF
        v3 ^= v0
        v2 = (v2 + v1) & 0xFFFFFFFFFFFFFFFF
        v1 = _rotl64(v1, 17) & 0xFFFFFFFFFFFFFFFF
        v1 ^= v2
        v2 = _rotl64(v2, 32) & 0xFFFFFFFFFFFFFFFF

    msg_len = len(msg)
    i = 0
    while i + 8 <= msg_len:
        m = int.from_bytes(msg[i:i + 8], 'little')
        v3 ^= m
        _round()
        _round()
        v0 ^= m
        i += 8

    b = (msg_len << 56) & 0xFFFFFFFFFFFFFFFF
    for j in range(i, msg_len):
        b |= msg[j] << (8 * (j - i))

    v3 ^= b
    _round()
    _round()
    v0 ^= b
    v2 ^= 0xFF
    for _ in range(4):
        _round()

    return (v0 ^ v1 ^ v2 ^ v3) & 0xFFFFFFFFFFFFFFFF


def jenkins_hash_64(data):
    if isinstance(data, str):
        data = data.encode('latin1')
    elif not isinstance(data, (bytes, bytearray, memoryview)):
        data = bytes(data)
    a, b = _jenkins_hash_little_2(data)
    return (a << 32) | b


def _jenkins_hash_little_2(data):
    length = len(data)
    a = (0xDEADBEEF + length) & 0xFFFFFFFF
    b = a
    c = a

    i = 0
    while i + 12 <= length:
        a = (a + (data[i] | (data[i + 1] << 8) | (data[i + 2] << 16) | (data[i + 3] << 24))) & 0xFFFFFFFF
        b = (b + (data[i + 4] | (data[i + 5] << 8) | (data[i + 6] << 16) | (data[i + 7] << 24))) & 0xFFFFFFFF
        c = (c + (data[i + 8] | (data[i + 9] << 8) | (data[i + 10] << 16) | (data[i + 11] << 24))) & 0xFFFFFFFF
        a, b, c = _jenkins_mix(a, b, c)
        i += 12

    k = data[i:]
    if len(k) == 0:
        return c & 0xFFFFFFFF, b & 0xFFFFFFFF

    if len(k) >= 12:
        c = (c + (k[11] << 24)) & 0xFFFFFFFF
    if len(k) >= 11:
        c = (c + (k[10] << 16)) & 0xFFFFFFFF
    if len(k) >= 10:
        c = (c + (k[9] << 8)) & 0xFFFFFFFF
    if len(k) >= 9:
        c = (c + k[8]) & 0xFFFFFFFF
    if len(k) >= 8:
        b = (b + (k[7] << 24)) & 0xFFFFFFFF
    if len(k) >= 7:
        b = (b + (k[6] << 16)) & 0xFFFFFFFF
    if len(k) >= 6:
        b = (b + (k[5] << 8)) & 0xFFFFFFFF
    if len(k) >= 5:
        b = (b + k[4]) & 0xFFFFFFFF
    if len(k) >= 4:
        a = (a + (k[3] << 24)) & 0xFFFFFFFF
    if len(k) >= 3:
        a = (a + (k[2] << 16)) & 0xFFFFFFFF
    if len(k) >= 2:
        a = (a + (k[1] << 8)) & 0xFFFFFFFF
    if len(k) >= 1:
        a = (a + k[0]) & 0xFFFFFFFF

    a, b, c = _jenkins_final(a, b, c)
    return c & 0xFFFFFFFF, b & 0xFFFFFFFF


def _jenkins_mix(a, b, c):
    a = (a - c) & 0xFFFFFFFF
    a = (a ^ _rotl32(c, 4)) & 0xFFFFFFFF
    c = (c + b) & 0xFFFFFFFF
    b = (b - a) & 0xFFFFFFFF
    b = (b ^ _rotl32(a, 6)) & 0xFFFFFFFF
    a = (a + c) & 0xFFFFFFFF
    c = (c - b) & 0xFFFFFFFF
    c = (c ^ _rotl32(b, 8)) & 0xFFFFFFFF
    b = (b + a) & 0xFFFFFFFF
    a = (a - c) & 0xFFFFFFFF
    a = (a ^ _rotl32(c, 16)) & 0xFFFFFFFF
    c = (c + b) & 0xFFFFFFFF
    b = (b - a) & 0xFFFFFFFF
    b = (b ^ _rotl32(a, 19)) & 0xFFFFFFFF
    a = (a + c) & 0xFFFFFFFF
    c = (c - b) & 0xFFFFFFFF
    c = (c ^ _rotl32(b, 4)) & 0xFFFFFFFF
    b = (b + a) & 0xFFFFFFFF
    return a, b, c


def _jenkins_final(a, b, c):
    c = (c ^ b) & 0xFFFFFFFF
    c = (c - _rotl32(b, 14)) & 0xFFFFFFFF
    a = (a ^ c) & 0xFFFFFFFF
    a = (a - _rotl32(c, 11)) & 0xFFFFFFFF
    b = (b ^ a) & 0xFFFFFFFF
    b = (b - _rotl32(a, 25)) & 0xFFFFFFFF
    c = (c ^ b) & 0xFFFFFFFF
    c = (c - _rotl32(b, 16)) & 0xFFFFFFFF
    a = (a ^ c) & 0xFFFFFFFF
    a = (a - _rotl32(c, 4)) & 0xFFFFFFFF
    b = (b ^ a) & 0xFFFFFFFF
    b = (b - _rotl32(a, 14)) & 0xFFFFFFFF
    c = (c ^ b) & 0xFFFFFFFF
    c = (c - _rotl32(b, 24)) & 0xFFFFFFFF
    return a, b, c


def _rotl32(v, n):
    return ((v << n) | (v >> (32 - n))) & 0xFFFFFFFF


def parse_match_string(s):
    if s == '':
        raise ValueError('EINVAL: empty match string')
    if s == '=':
        raise ValueError('EINVAL: missing field name')
    if s.startswith('='):
        raise ValueError('EINVAL: field name cannot start with =')

    eq = s.find('=')
    if eq < 0:
        raise ValueError('EINVAL: missing = separator')

    field = s[:eq]
    if field == '':
        raise ValueError('EINVAL: empty field name')
    if len(field) > 64:
        raise ValueError('EINVAL: field name too long')
    if field[0] >= '0' and field[0] <= '9':
        raise ValueError(f'EINVAL: invalid field name "{field}"')
    for i, c in enumerate(field):
        code = ord(c)
        if code != 0x5F and not (0x41 <= code <= 0x5A) and not (0x30 <= code <= 0x39):
            raise ValueError(f'EINVAL: invalid field name "{field}"')
    return s.encode('latin1')
