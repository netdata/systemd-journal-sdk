"""Platform adapters for file I/O primitives used by core readers/writers."""

import errno
import os
import threading


_IS_WINDOWS = os.name == 'nt'
_FALLBACK_IO_LOCK = threading.RLock()


def read_at(fd, size, offset):
    if size < 0 or offset < 0:
        raise ValueError('read_at bounds must be non-negative')
    if hasattr(os, 'pread'):
        return os.pread(fd, size, offset)
    with _FALLBACK_IO_LOCK:
        original = os.lseek(fd, 0, os.SEEK_CUR)
        try:
            os.lseek(fd, offset, os.SEEK_SET)
            chunks = []
            remaining = size
            while remaining > 0:
                chunk = os.read(fd, remaining)
                if not chunk:
                    break
                chunks.append(chunk)
                remaining -= len(chunk)
            return b''.join(chunks)
        finally:
            os.lseek(fd, original, os.SEEK_SET)


def write_all_at(fd, data, offset):
    if offset < 0:
        raise ValueError('write_all_at offset must be non-negative')
    view = memoryview(data)
    if hasattr(os, 'pwrite'):
        written = 0
        while written < len(view):
            n = os.pwrite(fd, view[written:], offset + written)
            if n <= 0:
                raise OSError('short positional write')
            written += n
        return
    with _FALLBACK_IO_LOCK:
        original = os.lseek(fd, 0, os.SEEK_CUR)
        try:
            os.lseek(fd, offset, os.SEEK_SET)
            written = 0
            while written < len(view):
                n = os.write(fd, view[written:])
                if n <= 0:
                    raise OSError('short positional write')
                written += n
        finally:
            os.lseek(fd, original, os.SEEK_SET)


def sync_parent_directory(path):
    parent = os.path.dirname(os.path.abspath(path)) or os.curdir
    return sync_directory(parent)


def sync_directory(path):
    if _IS_WINDOWS:
        return False
    flags = os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0)
    unsupported = {
        errno.EINVAL,
        getattr(errno, 'ENOTSUP', errno.EOPNOTSUPP),
        errno.EOPNOTSUPP,
    }
    try:
        fd = os.open(path, flags)
    except OSError as err:
        if err.errno in unsupported:
            return False
        raise
    try:
        try:
            os.fsync(fd)
        except OSError as err:
            if err.errno in unsupported:
                return False
            raise
        return True
    finally:
        os.close(fd)


def rename_requires_closed_file():
    return _IS_WINDOWS
