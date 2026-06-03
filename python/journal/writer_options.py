def _current_time_ms():
    import time
    return int(time.time() * 1000)


def _dedupe_entry_items(items):
    deduped = [items[0]]
    for i in range(1, len(items)):
        if items[i]['offset'] != deduped[-1]['offset']:
            deduped.append(items[i])
    return deduped


def _uuid_option(value, fallback):
    if value is None:
        return fallback
    if isinstance(value, str):
        value = value.replace('-', '')
        return bytes.fromhex(value)
    if isinstance(value, bytearray):
        value = bytes(value)
    if not isinstance(value, bytes) or len(value) != 16:
        raise ValueError('uuid options must be 16 bytes or 32 hex characters')
    return value


def _normalize_live_publish_every_entries(value):
    if value is None:
        return 1
    entries = int(value)
    if entries < 0:
        raise ValueError(f'invalid live_publish_every_entries: {value}')
    return entries
