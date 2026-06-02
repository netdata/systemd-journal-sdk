"""Optional platform adapters for lock and host-identity helpers."""

import errno
import os
import sys
import time
import uuid


_IS_WINDOWS = os.name == 'nt'
_IS_LINUX = sys.platform.startswith('linux')
_PROCESS_START_TOKEN = f'portable:{os.getpid()}:{time.monotonic_ns()}:{uuid.uuid4().hex}'


def lock_fd_exclusive(fd):
    if _IS_WINDOWS:
        import msvcrt
        return _with_fd_position(
            fd,
            lambda: msvcrt.locking(fd, msvcrt.LK_NBLCK, 1),
        )
    import fcntl
    return fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)


def unlock_fd(fd):
    if _IS_WINDOWS:
        import msvcrt
        try:
            return _with_fd_position(
                fd,
                lambda: msvcrt.locking(fd, msvcrt.LK_UNLCK, 1),
            )
        except OSError:
            return None
    import fcntl
    return fcntl.flock(fd, fcntl.LOCK_UN)


def _with_fd_position(fd, callback):
    try:
        original = os.lseek(fd, 0, os.SEEK_CUR)
    except OSError:
        original = None
    try:
        os.lseek(fd, 0, os.SEEK_SET)
        return callback()
    finally:
        if original is not None:
            try:
                os.lseek(fd, original, os.SEEK_SET)
            except OSError:
                pass


def boot_id_string():
    if not _IS_LINUX:
        return ''
    try:
        with open('/proc/sys/kernel/random/boot_id', 'r', encoding='ascii') as f:
            return f.read().strip()
    except OSError:
        return ''


def boot_id_bytes():
    text = boot_id_string().replace('-', '')
    try:
        return bytes.fromhex(text) if len(text) == 32 else None
    except ValueError:
        return None


def process_start_time(pid):
    pid = int(pid)
    if _IS_LINUX:
        try:
            with open(f'/proc/{pid}/stat', 'r', encoding='ascii') as f:
                text = f.read()
            end = text.rfind(')')
            if end < 0:
                raise OSError(f'cannot parse /proc/{pid}/stat')
            fields = text[end + 2:].split()
            if len(fields) < 20:
                raise OSError(f'cannot parse start time from /proc/{pid}/stat')
            return fields[19]
        except OSError:
            if pid != os.getpid():
                raise
    if pid == os.getpid():
        return _PROCESS_START_TOKEN
    raise OSError(f'process start time unavailable for pid {pid}')


def process_matches_start_time(pid, expected_start_time):
    try:
        actual = process_start_time(pid)
    except OSError:
        return process_is_alive(pid)
    if actual == expected_start_time:
        return True
    if str(actual).startswith('portable:') or str(expected_start_time).startswith('portable:'):
        return process_is_alive(pid)
    return False


def process_is_alive(pid):
    pid = int(pid)
    if pid <= 0:
        return False
    if pid == os.getpid():
        return True
    if _IS_WINDOWS:
        return _windows_process_is_alive(pid)
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError as err:
        if err.errno == errno.ESRCH:
            return False
        if err.errno == errno.EPERM:
            return True
        return True


def _windows_process_is_alive(pid):
    import ctypes

    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
    process_query_limited_information = 0x1000
    still_active = 259
    error_invalid_parameter = 87
    error_access_denied = 5

    handle = kernel32.OpenProcess(process_query_limited_information, False, int(pid))
    if not handle:
        err = ctypes.get_last_error()
        if err == error_invalid_parameter:
            return False
        if err == error_access_denied:
            return True
        return True
    try:
        exit_code = ctypes.c_ulong()
        if not kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
            return True
        return exit_code.value == still_active
    finally:
        kernel32.CloseHandle(handle)
