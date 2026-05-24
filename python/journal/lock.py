import os
import time


LOCK_VERSION = 'systemd-journal-sdk-lock-v1'
STALE_GRACE_SECONDS = 2.0


class WriterLock:
    def __init__(self, path, owner):
        self.path = path
        self.owner = owner

    @staticmethod
    def acquire(journal_path):
        lock_path = journal_path + '.lock'
        owner = _current_owner()
        while True:
            parent = os.path.dirname(lock_path)
            if parent:
                os.makedirs(parent, mode=0o750, exist_ok=True)
            try:
                fd = os.open(lock_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            except FileExistsError:
                stale, holder = _lock_file_is_stale(lock_path)
                if not stale:
                    raise BlockingIOError(f'journal writer lock held by {holder}')
                try:
                    os.unlink(lock_path)
                except FileNotFoundError:
                    pass
                continue
            try:
                _write_owner(fd, owner)
                return WriterLock(lock_path, owner)
            except Exception:
                try:
                    os.close(fd)
                except OSError:
                    pass
                finally:
                    try:
                        os.unlink(lock_path)
                    except FileNotFoundError:
                        pass
                raise

    def release(self):
        if not self.path:
            return
        try:
            owner = _read_owner(self.path)
        except FileNotFoundError:
            self.path = None
            return
        if owner == _current_owner():
            try:
                os.unlink(self.path)
            except FileNotFoundError:
                pass
        self.path = None


def _write_owner(fd, owner):
    text = (
        f'{LOCK_VERSION}\n'
        f'pid={owner["pid"]}\n'
        f'boot_id={owner["boot_id"]}\n'
        f'start_time={owner["start_time"]}\n'
    ).encode('utf-8')
    os.write(fd, text)
    os.fsync(fd)
    os.close(fd)


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

    if owner.get('boot_id') != _boot_id():
        return True, f'pid {owner.get("pid")} from previous boot'
    try:
        start_time = _process_start_time(owner['pid'])
    except OSError:
        return True, f'stale pid {owner.get("pid")}'
    if start_time != owner.get('start_time'):
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
    try:
        with open('/proc/sys/kernel/random/boot_id', 'r', encoding='ascii') as f:
            return f.read().strip()
    except OSError:
        return ''


def _process_start_time(pid):
    with open(f'/proc/{pid}/stat', 'r', encoding='ascii') as f:
        text = f.read()
    end = text.rfind(')')
    if end < 0:
        raise OSError(f'cannot parse /proc/{pid}/stat')
    fields = text[end + 2:].split()
    if len(fields) < 20:
        raise OSError(f'cannot parse start time from /proc/{pid}/stat')
    return fields[19]


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
