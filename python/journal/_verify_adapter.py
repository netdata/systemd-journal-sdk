"""Bytes-like adapter for bounded verification reads."""

from __future__ import annotations


class _AccessorBytesAdapter:
    def __init__(self, reader):
        self._reader = reader
        self.temp_copy_ranges = 0
        self.hmac_chunks = 0

    def __len__(self):
        return self._reader._visible_size()

    def __getitem__(self, key):
        if isinstance(key, slice):
            start, stop, step = key.indices(len(self))
            if step != 1:
                return bytes(self[i] for i in range(start, stop, step))
            if stop < start:
                stop = start
            self.temp_copy_ranges += 1
            return self._reader._read_bytes(start, stop - start)
        if key < 0:
            key += len(self)
        if key < 0 or key >= len(self):
            raise IndexError(key)
        return self._reader._u8(key)

    def update_hmac(self, hm, offset, size, *, chunk_size=1 << 20):
        offset = int(offset)
        remaining = int(size)
        if remaining < 0:
            raise ValueError("negative HMAC range size")
        while remaining:
            chunk = min(remaining, chunk_size)
            hm.update(self._reader._read_bytes(offset, chunk))
            self.hmac_chunks += 1
            offset += chunk
            remaining -= chunk
