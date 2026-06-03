DEFAULT_JOURNAL_FILE_MODE = 0o640


def _normalize_file_mode(opts):
    value = opts.get('file_mode')
    if value is None:
        value = opts.get('fileMode')
    if value is None:
        return DEFAULT_JOURNAL_FILE_MODE
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError(f'invalid journal file mode: {value!r}')
    if value < 0 or value > 0o777:
        raise ValueError(f'invalid journal file mode: {value!r}')
    return value
