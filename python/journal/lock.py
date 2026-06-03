import os
import time
from contextlib import suppress

from ._platform import (
    boot_id_string,
    lock_fd_exclusive,
    process_matches_start_time,
    process_start_time,
    unlock_fd,
)


LOCK_VERSION = 'systemd-journal-sdk-lock-v1'
STALE_GRACE_SECONDS = 2.0


class WriterLock:
    def __init__(self, path, owner, fd=None):
        self.path = path
        self.owner = owner
        self.fd = fd

    @staticmethod
    def acquire(journal_path):
        lock_path = journal_path + '.lock'
        owner = _current_owner()
        while True:
            parent = os.path.dirname(lock_path)
            if parent:
                os.makedirs(parent, mode=0o750, exist_ok=True)
            fd = None
            try:
                fd = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
            except FileExistsError:
                stale, holder = _lock_file_is_stale(lock_path)
                if not stale:
                    raise BlockingIOError(f'journal writer lock held by {holder}')
                with suppress(FileNotFoundError):
                    os.unlink(lock_path)
                continue
            try:
                lock_fd_exclusive(fd)
                _write_owner(fd, owner)
                return WriterLock(lock_path, owner, fd)
            except Exception:
                if fd is not None:
                    with suppress(OSError):
                        os.close(fd)
                with suppress(FileNotFoundError):
                    os.unlink(lock_path)
                raise

    def release(self):
        if not self.path:
            return
        should_unlink = False
        try:
            owner = _read_owner(self.path)
        except FileNotFoundError:
            self._close_fd()
            self.path = None
            return
        if owner == _current_owner():
            should_unlink = True
        self._close_fd()
        if should_unlink:
            with suppress(FileNotFoundError):
                os.unlink(self.path)
        self.path = None

    def _close_fd(self):
        if self.fd is None:
            return
        fd = self.fd
        self.fd = None
        try:
            unlock_fd(fd)
        finally:
            os.close(fd)


def _write_owner(fd, owner):
    text = (
        f'{LOCK_VERSION}\n'
        f'pid={owner["pid"]}\n'
        f'boot_id={owner["boot_id"]}\n'
        f'start_time={owner["start_time"]}\n'
    ).encode('utf-8')
    os.ftruncate(fd, 0)
    os.lseek(fd, 0, os.SEEK_SET)
    written = 0
    while written < len(text):
        n = os.write(fd, text[written:])
        if n <= 0:
            raise OSError('short lock metadata write')
        written += n
    os.fsync(fd)


def _lock_file_is_stale(path):
    try:
        owner = _read_owner(path)
    except Exception:
        try:
            age = time.time() - os.stat(path).st_mtime
        except FileNotFoundError:
            return True, 'missing lock'
        if age <= STALE_GRACE_SECONDS:
            return False, 'partially-created lock'
        return True, 'malformed stale lock'

    current_boot_id = _boot_id()
    if current_boot_id and owner.get('boot_id') and owner.get('boot_id') != current_boot_id:
        return True, f'pid {owner.get("pid")} from previous boot'
    if not process_matches_start_time(owner['pid'], owner.get('start_time')):
        return True, f'stale pid {owner.get("pid")}'
    return False, f'pid {owner.get("pid")}'


def _current_owner():
    pid = os.getpid()
    return {
        'pid': pid,
        'boot_id': _boot_id(),
        'start_time': _process_start_time(pid),
    }


def _boot_id():
    return boot_id_string()


def _process_start_time(pid):
    return process_start_time(pid)


def _read_owner(path):
    with open(path, 'r', encoding='utf-8') as f:
        lines = [line.strip() for line in f.readlines()]
    if len(lines) < 4 or lines[0] != LOCK_VERSION:
        raise ValueError('invalid lock metadata')
    owner = {}
    for line in lines[1:]:
        if '=' not in line:
            continue
        key, value = line.split('=', 1)
        if key == 'pid':
            owner[key] = int(value)
        elif key in ('boot_id', 'start_time'):
            owner[key] = value
    if owner.get('pid', 0) <= 0 or not owner.get('start_time'):
        raise ValueError('incomplete lock metadata')
    return owner
