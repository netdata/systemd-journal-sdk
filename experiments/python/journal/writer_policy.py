FIELD_NAME_POLICY_JOURNALD = 'journald'
FIELD_NAME_POLICY_RAW = 'raw'
FIELD_NAME_POLICY_JOURNAL_APP = 'journal-app'


def _normalize_field_name_policy(value):
    if value is None or value == '':
        return FIELD_NAME_POLICY_JOURNALD
    if value == FIELD_NAME_POLICY_JOURNALD:
        return FIELD_NAME_POLICY_JOURNALD
    if value == FIELD_NAME_POLICY_RAW:
        return FIELD_NAME_POLICY_RAW
    if value == FIELD_NAME_POLICY_JOURNAL_APP:
        return FIELD_NAME_POLICY_JOURNAL_APP
    raise ValueError(f'unsupported field name policy: {value}')


def _writer_policy_for_log_policy(policy):
    return FIELD_NAME_POLICY_RAW if _normalize_field_name_policy(policy) == FIELD_NAME_POLICY_RAW else FIELD_NAME_POLICY_JOURNALD


def _prepare_fields_for_policy(fields, policy):
    policy = _normalize_field_name_policy(policy)
    if not fields:
        raise ValueError('empty entry')
    if policy == FIELD_NAME_POLICY_JOURNAL_APP:
        filtered = []
        for field in fields:
            try:
                _validate_field_name_for_policy(field['name'], policy)
            except ValueError:
                continue
            filtered.append(field)
        if not filtered:
            raise ValueError('empty entry')
        return filtered
    for field in fields:
        _validate_field_name_for_policy(field['name'], policy)
    return fields


def _prepare_raw_payloads_for_policy(payloads, policy):
    policy = _normalize_field_name_policy(policy)
    if not payloads:
        raise ValueError('empty entry')
    prepared = []
    for payload in payloads:
        payload = _raw_payload_bytes(payload)
        field_name = _raw_payload_field_name(payload)
        if policy == FIELD_NAME_POLICY_JOURNAL_APP:
            try:
                _validate_field_name_for_policy(field_name, policy)
            except ValueError:
                continue
        else:
            _validate_field_name_for_policy(field_name, policy)
        prepared.append(payload)
    if not prepared:
        raise ValueError('empty entry')
    return prepared


def _raw_payload_bytes(payload):
    if isinstance(payload, bytes):
        return payload
    if isinstance(payload, (bytearray, memoryview)):
        return bytes(payload)
    if isinstance(payload, str):
        return payload.encode('utf-8')
    return bytes(payload)


def _raw_payload_field_name(payload):
    eq = payload.find(b'=')
    if eq < 0:
        raise ValueError('invalid raw payload: missing field separator')
    if eq == 0:
        raise ValueError('invalid field name: empty')
    return payload[:eq]


def _validate_field_name_for_policy(name, policy=FIELD_NAME_POLICY_JOURNALD):
    policy = _normalize_field_name_policy(policy)
    if policy == FIELD_NAME_POLICY_RAW:
        return _validate_raw_field_name(name)
    return _validate_journald_field_name(name, allow_protected=(policy == FIELD_NAME_POLICY_JOURNALD))


def _field_name_bytes(name):
    if isinstance(name, bytes):
        return name
    if isinstance(name, (bytearray, memoryview)):
        return bytes(name)
    return str(name).encode('utf-8')


def _field_name_for_error(name):
    if isinstance(name, (bytes, bytearray, memoryview)):
        return bytes(name).decode('utf-8', errors='replace')
    return str(name)


def _validate_raw_field_name(name):
    data = _field_name_bytes(name)
    if len(data) == 0:
        raise ValueError('invalid field name: empty')
    if b'=' in data:
        raise ValueError(f"invalid field name: contains '=': {_field_name_for_error(name)}")


def _validate_journald_field_name(name, allow_protected=True):
    data = _field_name_bytes(name)
    display = _field_name_for_error(name)
    if len(data) == 0:
        raise ValueError('invalid field name: empty')
    if len(data) > 64:
        raise ValueError(f'invalid field name: too long ({len(data)})')
    if not allow_protected and data[0] == 0x5F:
        raise ValueError(f'invalid field name: protected: {display}')
    if 0x30 <= data[0] <= 0x39:
        raise ValueError(f'invalid field name: starts with digit: {display}')
    for i, code in enumerate(data):
        if code != 0x5F and not (0x41 <= code <= 0x5A) and not (0x30 <= code <= 0x39):
            raise ValueError(f'invalid field name: bad char at {i}: {display}')
