"""Rust-compatible field-name remapping for high-level directory writers."""

from __future__ import annotations

import hashlib


REMAPPING_MARKER = 'ND_REMAPPING'
REMAPPED_PREFIX = 'ND_'
MASK64 = (1 << 64) - 1

LOWERCASE = 0
UPPERCASE = 1
DOT = 2
UNDERSCORE = 3
HYPHEN = 4

TOKEN_LOWERCASE = 0
TOKEN_UPPERCASE = 1
TOKEN_CAPITALIZED = 2

FIELD_LOWERCASE = 0
FIELD_UPPERCASE = 1
FIELD_LOWER_CAMEL = 2
FIELD_UPPER_CAMEL = 3
FIELD_EMPTY = 4

SEP_DOT = 0
SEP_HYPHEN = 1
SEP_UNDERSCORE = 2


def is_systemd_compatible_log_field_name(name):
    if not isinstance(name, str) or len(name) == 0 or len(name) > 64:
        return False
    first = ord(name[0])
    if first < 0x41 or first > 0x5A:
        return False
    for ch in name:
        c = ord(ch)
        if c == 0x5F or 0x41 <= c <= 0x5A or 0x30 <= c <= 0x39:
            continue
        return False
    return True


def remap_fields(fields, registry):
    out = []
    mappings = []
    pending = {}

    for field in fields:
        name = field['name']
        if is_systemd_compatible_log_field_name(name):
            out.append(field)
            continue

        mapped = registry.get(name) or pending.get(name)
        if mapped is None:
            mapped = encode_remapped_field_name(name)
            pending[name] = mapped
            mappings.append({'original': name, 'mapped': mapped})

        remapped = dict(field)
        remapped['name'] = mapped
        out.append(remapped)

    return out, mappings


def encode_remapped_field_name(field_name):
    if isinstance(field_name, bytes):
        raw = field_name
    else:
        raw = str(field_name).encode('utf-8')

    encoded = _rdp_encode(raw)
    if encoded is None:
        return _md5_fallback(raw)

    if _has_checksum(encoded):
        compressed = encoded[:2] + _compress_runs(encoded[2:])
    else:
        compressed = _compress_runs(encoded)

    normalized = raw.decode('utf-8').upper().replace('.', '_').replace('-', '_')
    if normalized.startswith('RESOURCE_ATTRIBUTES_'):
        normalized = 'RA_' + normalized[len('RESOURCE_ATTRIBUTES_'):]
    elif normalized.startswith('LOG_ATTRIBUTES_'):
        normalized = 'LA_' + normalized[len('LOG_ATTRIBUTES_'):]
    elif normalized.startswith('LOG_BODY_'):
        normalized = 'LB_' + normalized[len('LOG_BODY_'):]

    result = 'ND' + compressed.upper() + '_' + normalized
    if len(result) > 64:
        return _md5_fallback(raw)
    return result


def _md5_fallback(raw):
    return REMAPPED_PREFIX + hashlib.md5(raw).hexdigest().upper()


def _rdp_encode(raw):
    tokens = _tokenize(raw)
    if tokens is None:
        return None
    return _encode_nodes(raw.decode('utf-8'), _parse_tokens(tokens))


def _char_kind(c):
    if 0x61 <= c <= 0x7A:
        return LOWERCASE
    if 0x41 <= c <= 0x5A or 0x30 <= c <= 0x39:
        return UPPERCASE
    if c == 0x2E:
        return DOT
    if c == 0x5F:
        return UNDERSCORE
    if c == 0x2D:
        return HYPHEN
    return None


def _tokenize(raw):
    if len(raw) == 0:
        return []

    tokens = []
    start = 0
    previous = None
    first = None
    has_lowercase = False
    has_uppercase = False

    for i, byte in enumerate(raw):
        current = _char_kind(byte)
        if current is None:
            return None
        if previous is None:
            first = current
            previous = current
            continue

        if previous in (DOT, UNDERSCORE, HYPHEN):
            should_split = True
        elif current in (DOT, UNDERSCORE, HYPHEN):
            should_split = True
        elif previous == UPPERCASE and current == UPPERCASE:
            next_kind = _char_kind(raw[i + 1]) if i + 1 < len(raw) else None
            should_split = next_kind == LOWERCASE
        elif previous == LOWERCASE and current == LOWERCASE:
            should_split = False
        elif previous == UPPERCASE and current == LOWERCASE:
            should_split = has_uppercase and has_lowercase
        else:
            should_split = True

        if should_split:
            tokens.append(_create_token(first, has_lowercase, has_uppercase, start, i))
            start = i
            first = current
            has_lowercase = False
            has_uppercase = False
        else:
            if current == LOWERCASE:
                has_lowercase = True
            elif current == UPPERCASE:
                has_uppercase = True
        previous = current

    if start < len(raw):
        tokens.append(_create_token(first, has_lowercase, has_uppercase, start, len(raw)))
    return tokens


def _create_token(first, has_lowercase, _has_uppercase, start, end):
    if first == LOWERCASE:
        return {'word': True, 'kind': TOKEN_LOWERCASE, 'start': start, 'end': end}
    if first == UPPERCASE:
        return {
            'word': True,
            'kind': TOKEN_CAPITALIZED if has_lowercase else TOKEN_UPPERCASE,
            'start': start,
            'end': end,
        }
    if first == DOT:
        return {'word': False, 'sep': SEP_DOT}
    if first == HYPHEN:
        return {'word': False, 'sep': SEP_HYPHEN}
    return {'word': False, 'sep': SEP_UNDERSCORE}


def _parse_tokens(tokens):
    nodes = []
    if tokens and not tokens[0]['word']:
        nodes.append({'field': True, 'type': FIELD_EMPTY})

    builder = None
    for i, token in enumerate(tokens):
        if not token['word']:
            if builder is not None:
                nodes.append({'field': True, 'type': builder['type']})
                builder = None
            nodes.append({'field': False, 'sep': token['sep']})
            if i + 1 >= len(tokens) or not tokens[i + 1]['word']:
                nodes.append({'field': True, 'type': FIELD_EMPTY})
            continue

        if builder is not None:
            if _can_add(builder['type'], token['kind']):
                builder['extended'] = True
                continue
            if (
                builder['type'] == FIELD_LOWERCASE and
                not builder['extended'] and
                token['kind'] == TOKEN_CAPITALIZED
            ):
                builder['type'] = FIELD_LOWER_CAMEL
                builder['extended'] = True
                continue
            nodes.append({'field': True, 'type': builder['type']})

        builder = {'type': _field_type_for_token(token['kind']), 'extended': False}

    if builder is not None:
        nodes.append({'field': True, 'type': builder['type']})
    return nodes


def _can_add(field_type, token_type):
    return (
        (field_type == FIELD_LOWERCASE and token_type == TOKEN_LOWERCASE) or
        (field_type == FIELD_UPPERCASE and token_type == TOKEN_UPPERCASE) or
        (field_type == FIELD_LOWER_CAMEL and token_type == TOKEN_CAPITALIZED) or
        (field_type == FIELD_UPPER_CAMEL and token_type == TOKEN_CAPITALIZED)
    )


def _field_type_for_token(token_type):
    if token_type == TOKEN_LOWERCASE:
        return FIELD_LOWERCASE
    if token_type == TOKEN_UPPERCASE:
        return FIELD_UPPERCASE
    return FIELD_UPPER_CAMEL


def _encode_nodes(source, nodes):
    has_camel = any(
        node['field'] and node['type'] in (FIELD_LOWER_CAMEL, FIELD_UPPER_CAMEL)
        for node in nodes
    )
    out = _checksum(source) if has_camel else ''

    i = 0
    while i < len(nodes):
        node = nodes[i]
        if not node['field']:
            i += 1
            continue

        next_is_separator = i + 1 < len(nodes) and not nodes[i + 1]['field']
        next_is_field = i + 1 < len(nodes) and nodes[i + 1]['field']
        sep = nodes[i + 1]['sep'] if next_is_separator else SEP_DOT
        out += chr(_pair_char(node['type'], next_is_separator, next_is_field, sep))
        i += 2 if next_is_separator else 1

    return out


def _pair_char(field_type, next_is_separator, next_is_field, sep):
    base = ord('a')
    if field_type == FIELD_LOWER_CAMEL:
        base = ord('f')
    elif field_type == FIELD_UPPER_CAMEL:
        base = ord('k')
    elif field_type == FIELD_UPPERCASE:
        base = ord('p')
    elif field_type == FIELD_EMPTY:
        base = ord('u')

    if next_is_separator:
        if sep == SEP_DOT:
            return base
        if sep == SEP_UNDERSCORE:
            return base + 1
        return base + 2
    if next_is_field and field_type != FIELD_EMPTY:
        return base + 3
    return base + 4


def _checksum(source):
    raw = source.encode('utf-8') + b'\xff'
    value = _sip_hash_13_zero(raw)
    first = (value // 36) % 36
    second = value % 36
    return chr(_checksum_char(first)) + chr(_checksum_char(second))


def _checksum_char(index):
    return ord('A') + index if index < 26 else ord('0') + index - 26


def _has_checksum(encoded):
    return bool(encoded) and ('A' <= encoded[0] <= 'Z' or '0' <= encoded[0] <= '9')


def _compress_runs(value):
    out = []
    i = 0
    while i < len(value):
        ch = value[i]
        count = 1
        while i + count < len(value) and value[i + count] == ch:
            count += 1
        if count <= 2:
            out.append(ch * count)
        else:
            remaining = count
            while remaining > 0:
                if remaining > 9:
                    out.append('9' + ch)
                    remaining -= 9
                elif remaining > 2:
                    out.append(str(remaining) + ch)
                    remaining = 0
                else:
                    out.append(ch * remaining)
                    remaining = 0
        i += count
    return ''.join(out)


def _sip_hash_13_zero(msg):
    v0 = 0x736F6D6570736575
    v1 = 0x646F72616E646F6D
    v2 = 0x6C7967656E657261
    v3 = 0x7465646279746573

    def sip_round():
        nonlocal v0, v1, v2, v3
        v0 = (v0 + v1) & MASK64
        v1 = _rotl64(v1, 13) ^ v0
        v0 = _rotl64(v0, 32)
        v2 = (v2 + v3) & MASK64
        v3 = _rotl64(v3, 16) ^ v2
        v0 = (v0 + v3) & MASK64
        v3 = _rotl64(v3, 21) ^ v0
        v2 = (v2 + v1) & MASK64
        v1 = _rotl64(v1, 17) ^ v2
        v2 = _rotl64(v2, 32)

    offset = 0
    while offset + 8 <= len(msg):
        m = int.from_bytes(msg[offset:offset + 8], 'little')
        v3 ^= m
        sip_round()
        v0 ^= m
        offset += 8

    b = len(msg) << 56
    for i, byte in enumerate(msg[offset:]):
        b |= byte << (8 * i)

    v3 ^= b
    sip_round()
    v0 ^= b
    v2 ^= 0xFF
    sip_round()
    sip_round()
    sip_round()

    return (v0 ^ v1 ^ v2 ^ v3) & MASK64


def _rotl64(value, bits):
    return ((value << bits) | (value >> (64 - bits))) & MASK64
