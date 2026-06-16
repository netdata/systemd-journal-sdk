import mmap
import os

from ._platform_io import (
    read_at as _read_fd_at,
    write_all_at as _write_fd_at,
)


class _MappedArena:
    def __init__(self, fd, size):
        self._fd = fd
        self._mmap = None
        self._size = 0
        self.resize(size)

    def resize(self, size):
        size = int(size)
        if size <= 0:
            raise ValueError('mapped arena size must be positive')
        if self._mmap is not None and size == self._size:
            return
        os.ftruncate(self._fd, size)
        if self._mmap is None:
            self._mmap = mmap.mmap(self._fd, size, access=mmap.ACCESS_WRITE)
        elif size != self._size:
            try:
                self._mmap.resize(size)
            except Exception:
                self._mmap.flush()
                self._mmap.close()
                self._mmap = None
                self._mmap = mmap.mmap(self._fd, size, access=mmap.ACCESS_WRITE)
        self._size = size

    def read_at(self, offset, size):
        end = int(offset) + int(size)
        if offset < 0 or size < 0 or end > self._size:
            raise ValueError('mapped arena read out of bounds')
        return self._mmap[offset:end]

    def write_at(self, offset, data):
        end = int(offset) + len(data)
        if offset < 0 or end > self._size:
            raise ValueError('mapped arena write out of bounds')
        self._mmap[offset:end] = data

    def flush(self):
        if self._mmap is not None:
            self._mmap.flush()

    def close(self):
        if self._mmap is None:
            return
        self._mmap.close()
        self._mmap = None


class _FileArena:
    def __init__(self, fd, size):
        self._fd = fd
        self._size = 0
        self.resize(size)

    def resize(self, size):
        size = int(size)
        if size <= 0:
            raise ValueError('file arena size must be positive')
        if size != self._size:
            os.ftruncate(self._fd, size)
            self._size = size

    def read_at(self, offset, size):
        end = int(offset) + int(size)
        if offset < 0 or size < 0 or end > self._size:
            raise ValueError('file arena read out of bounds')
        return _read_fd_at(self._fd, size, offset)

    def write_at(self, offset, data):
        end = int(offset) + len(data)
        if offset < 0 or end > self._size:
            raise ValueError('file arena write out of bounds')
        _write_fd_at(self._fd, data, offset)

    def flush(self):
        return None

    def close(self):
        return None
