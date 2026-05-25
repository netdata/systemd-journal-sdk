# Journal file verification.
# Validates structural integrity of unsealed journal files.
# Sealed FSS tag/HMAC verification is not yet implemented.

from .reader import FileReader
from .entry import parse_entry_object, parse_data_object
from .header import INCOMPATIBLE_COMPACT


class VerificationError(Exception):
    """Raised when a journal file fails structural integrity verification."""
    pass


def verify_file(path):
    """Validate the structural integrity of a journal file.

    Opens the file (decompressing .zst if needed), validates the header,
    and walks all entries and their referenced data objects strictly.
    Any parse or decompression error is reported as a VerificationError.

    For sealed journals, tag/HMAC verification is not yet implemented.
    """
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

        for offset in r._entry_offsets:
            # Parse entry object strictly
            try:
                e = parse_entry_object(buf, offset, compact)
            except Exception as err:
                raise VerificationError(
                    f"journal verification failed: corrupt entry object at offset {offset}: {err}"
                ) from err

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
