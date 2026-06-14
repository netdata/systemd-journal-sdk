"""Bounded reader byte access for journal files."""

from __future__ import annotations

import collections
import contextlib
import mmap
import os

from ._platform_io import read_at, read_at_uses_pread


READER_ACCESS_AUTO = "auto"
READER_ACCESS_MMAP = "mmap"
READER_ACCESS_READ_AT = "read-at"
READER_BOUNDS_LIVE = "live"
READER_BOUNDS_SNAPSHOT = "snapshot"

DEFAULT_WINDOW_SIZE = 32 * 1024 * 1024
DEFAULT_MAX_WINDOWS = 4
DEFAULT_MAX_ROW_ARENA_BYTES = 256 * 1024 * 1024
DEFAULT_ROW_ARENA_SEGMENT_BYTES = 1024 * 1024


class ReaderOptions:
    def __init__(
        self,
        access_mode=READER_ACCESS_AUTO,
        window_size=DEFAULT_WINDOW_SIZE,
        max_windows=DEFAULT_MAX_WINDOWS,
        max_row_arena_bytes=DEFAULT_MAX_ROW_ARENA_BYTES,
        max_retired_windows=None,
        row_arena_segment_bytes=DEFAULT_ROW_ARENA_SEGMENT_BYTES,
        bounds=READER_BOUNDS_LIVE,
    ):
        self.access_mode = _normalize_access_mode(access_mode)
        self.window_size = int(window_size)
        self.max_windows = int(max_windows)
        self.max_row_arena_bytes = int(max_row_arena_bytes)
        self.max_retired_windows = (
            self.max_windows if max_retired_windows is None else int(max_retired_windows)
        )
        self.row_arena_segment_bytes = int(row_arena_segment_bytes)
        self.bounds = _normalize_bounds(bounds)
        self._validate()

    def _validate(self):
        if self.window_size <= 0:
            raise ValueError("window_size must be positive")
        if self.max_windows <= 0:
            raise ValueError("max_windows must be positive")
        if self.max_row_arena_bytes < 0:
            raise ValueError("max_row_arena_bytes must not be negative")
        if self.max_retired_windows < 0:
            raise ValueError("max_retired_windows must not be negative")
        if self.row_arena_segment_bytes <= 0:
            raise ValueError("row_arena_segment_bytes must be positive")


def default_reader_options():
    return ReaderOptions()


def _normalize_access_mode(access_mode):
    text = str(access_mode).lower().replace("_", "-")
    aliases = {
        "auto": READER_ACCESS_AUTO,
        "mmap": READER_ACCESS_MMAP,
        "readat": READER_ACCESS_READ_AT,
        "read-at": READER_ACCESS_READ_AT,
        "pread": READER_ACCESS_READ_AT,
    }
    try:
        return aliases[text]
    except KeyError as err:
        raise ValueError(f"unsupported reader access mode: {access_mode}") from err


def _normalize_bounds(bounds):
    text = str(bounds).lower().replace("_", "-")
    aliases = {
        "live": READER_BOUNDS_LIVE,
        "snapshot": READER_BOUNDS_SNAPSHOT,
    }
    try:
        return aliases[text]
    except KeyError as err:
        raise ValueError(f"unsupported reader bounds mode: {bounds}") from err


def open_reader_accessor(path, options=None):
    opts = options if isinstance(options, ReaderOptions) else ReaderOptions() if options is None else options
    if not isinstance(opts, ReaderOptions):
        raise TypeError("options must be ReaderOptions or None")

    fd = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0))
    try:
        if opts.access_mode == READER_ACCESS_READ_AT:
            return _ReadAtAccessor(fd, opts, selected=READER_ACCESS_READ_AT)
        if opts.access_mode == READER_ACCESS_MMAP:
            return _MmapAccessor(fd, opts, selected=READER_ACCESS_MMAP)
        try:
            return _MmapAccessor(fd, opts, selected=READER_ACCESS_MMAP)
        except Exception as mmap_err:
            accessor = _ReadAtAccessor(
                fd,
                opts,
                selected=READER_ACCESS_READ_AT,
                fallback_reason=str(mmap_err),
            )
            return accessor
    except Exception:
        with contextlib.suppress(Exception):
            os.close(fd)
        raise


class _StatsMixin:
    def selected_access_mode(self):
        return self._selected

    def stats(self):
        return {
            "selected_backend": self._selected,
            "fallback_reason": self._fallback_reason,
            "visible_size": self._visible_size,
            "window_size": self._options.window_size,
            "max_windows": self._options.max_windows,
            "current_windows": len(self._windows),
            "mapped_bytes": self._mapped_bytes(),
            "read_buffer_bytes": self._read_buffer_bytes(),
            "row_pinned_windows": sum(1 for window in self._windows.values() if window.row_pinned),
            "retired_windows": len(getattr(self, "_retired", [])),
            "retired_bytes": sum(window.length for window in getattr(self, "_retired", [])),
            "row_arena_current_bytes": self._row_arena.current_bytes,
            "row_arena_peak_bytes": self._row_arena.peak_bytes,
            "row_arena_limit_bytes": self._row_arena.limit_bytes,
            "row_arena_segment_bytes": self._row_arena.segment_bytes,
            "row_arena_active_segments": len(self._row_arena.segments),
            "read_at_uses_pread": read_at_uses_pread(),
            "temp_copy_count": self._temp_copy_count,
            "window_miss_count": self._window_miss_count,
            "eviction_count": self._eviction_count,
        }


class _BaseAccessor(_StatsMixin):
    def __init__(self, fd, options, selected, fallback_reason=None):
        self._fd = fd
        self._options = options
        self._selected = selected
        self._fallback_reason = fallback_reason
        self._visible_size = os.fstat(fd).st_size
        self._windows = collections.OrderedDict()
        self._scratch = None
        self._closed = False
        self._temp_copy_count = 0
        self._window_miss_count = 0
        self._eviction_count = 0
        self._row_arena = _RowArena(options.max_row_arena_bytes, options.row_arena_segment_bytes)

    def size(self):
        return self._visible_size

    def fd(self):
        return self._fd

    def bounds_mode(self):
        return self._options.bounds

    def read_bytes(self, offset, size):
        return bytes(self.temp_view(offset, size))

    def row_bytes(self, data):
        return self._row_arena.append(data)

    def u8(self, offset):
        return self.temp_view(offset, 1)[0]

    def u32(self, offset):
        return int.from_bytes(self.temp_view(offset, 4), "little")

    def u64(self, offset):
        return int.from_bytes(self.temp_view(offset, 8), "little")

    def clear_row(self):
        for window in list(self._windows.values()):
            window.row_pinned = False
        self._row_arena.clear()
        self._evict_to_budget()
        self._retry_retired()

    def snapshot_visible_bounds(self):
        return self._visible_size

    def restore_visible_bounds(self, snapshot):
        self._visible_size = int(snapshot)

    def refresh_visible_bounds(self):
        if self._options.bounds == READER_BOUNDS_SNAPSHOT:
            return False, self._visible_size
        size = os.fstat(self._fd).st_size
        changed = size != self._visible_size
        self._visible_size = size
        return changed, size

    def close(self):
        if self._closed:
            return
        self.clear_row()
        for base in list(self._windows):
            self._close_window(base)
        self._retry_retired()
        with contextlib.suppress(Exception):
            os.close(self._fd)
        self._closed = True

    def _check_range(self, offset, size):
        offset = int(offset)
        size = int(size)
        if offset < 0 or size < 0:
            raise ValueError("negative offset or size")
        if offset + size > self._visible_size:
            raise ValueError("read exceeds visible file bounds")
        return offset, size

    def _scratch_view(self, offset, size):
        offset, size = self._check_range(offset, size)
        data = read_at(self._fd, size, offset)
        if len(data) != size:
            raise ValueError("short read")
        self._scratch = data
        self._temp_copy_count += 1
        return memoryview(data)

    def _mapped_bytes(self):
        return 0

    def _read_buffer_bytes(self):
        return 0

    def _retry_retired(self):
        return None


class _MmapWindow:
    def __init__(self, base, length, mapping):
        self.base = base
        self.length = length
        self.mapping = mapping
        self.view = memoryview(mapping)
        self.row_pinned = False


class _MmapAccessor(_BaseAccessor):
    def __init__(self, fd, options, selected, fallback_reason=None):
        super().__init__(fd, options, selected, fallback_reason)
        self._granularity = int(getattr(mmap, "ALLOCATIONGRANULARITY", mmap.PAGESIZE))
        self._retired = []
        if self._visible_size > 0:
            self.temp_view(0, min(self._visible_size, min(options.window_size, 1)))

    def temp_view(self, offset, size):
        return self._view(offset, size, row=False)

    def row_view(self, offset, size):
        try:
            return self._view(offset, size, row=True)
        except RuntimeError:
            return self._row_arena.append(bytes(self.temp_view(offset, size)))

    def _view(self, offset, size, row):
        offset, size = self._check_range(offset, size)
        if size == 0:
            return memoryview(b"")
        if size > self._options.window_size:
            if row:
                return self._row_arena.append(bytes(self._scratch_view(offset, size)))
            return self._scratch_view(offset, size)

        base = (offset // self._granularity) * self._granularity
        window = self._windows.get(base)
        if window is None or offset + size > window.base + window.length:
            if window is not None and window.row_pinned:
                if row:
                    return self._row_arena.append(bytes(self._scratch_view(offset, size)))
                return self._scratch_view(offset, size)
            if len(self._windows) >= self._options.max_windows and self._all_windows_row_pinned():
                if row:
                    return self._row_arena.append(bytes(self._scratch_view(offset, size)))
                return self._scratch_view(offset, size)
            window = self._map_window(base, offset, size)
        else:
            self._windows.move_to_end(base)
        if row:
            window.row_pinned = True
        start = offset - window.base
        return window.view[start:start + size]

    def _map_window(self, base, offset, size):
        existing = self._windows.get(base)
        if existing is not None:
            if existing.row_pinned:
                raise RuntimeError("cannot replace row-pinned mmap window")
            self._close_window(base)
            self._eviction_count += 1
        else:
            self._evict_to_budget(extra_needed=1)
        length = min(self._options.window_size + (offset - base), self._visible_size - base)
        if offset + size > base + length:
            length = offset + size - base
        mapping = mmap.mmap(self._fd, length, access=mmap.ACCESS_READ, offset=base)
        window = _MmapWindow(base, length, mapping)
        self._windows[base] = window
        self._window_miss_count += 1
        self._evict_to_budget()
        return window

    def _all_windows_row_pinned(self):
        return bool(self._windows) and all(window.row_pinned for window in self._windows.values())

    def _evict_to_budget(self, extra_needed=0):
        while len(self._windows) + extra_needed > self._options.max_windows:
            for base, window in list(self._windows.items()):
                if not window.row_pinned:
                    self._close_window(base)
                    self._eviction_count += 1
                    break
            else:
                return

    def _close_window(self, base):
        window = self._windows.pop(base, None)
        if window is None:
            return
        with contextlib.suppress(Exception):
            window.view.release()
        try:
            window.mapping.close()
        except BufferError:
            self._retire_window(window)

    def _retire_window(self, window):
        self._retired.append(window)
        if len(self._retired) > self._options.max_retired_windows:
            raise RuntimeError("too many retired mmap windows; row views were retained past row lifetime")

    def _retry_retired(self):
        remaining = []
        for window in self._retired:
            with contextlib.suppress(Exception):
                window.view.release()
            try:
                window.mapping.close()
            except BufferError:
                remaining.append(window)
        self._retired = remaining

    def _mapped_bytes(self):
        return sum(window.length for window in self._windows.values())


class _ReadAtWindow:
    def __init__(self, base, data):
        self.base = base
        self.data = data
        self.length = len(data)
        self.view = memoryview(data)
        self.row_pinned = False


class _ReadAtAccessor(_BaseAccessor):
    def temp_view(self, offset, size):
        return self._view(offset, size, row=False)

    def row_view(self, offset, size):
        try:
            return self._view(offset, size, row=True)
        except RuntimeError:
            return self._row_arena.append(bytes(self.temp_view(offset, size)))

    def _view(self, offset, size, row):
        offset, size = self._check_range(offset, size)
        if size == 0:
            return memoryview(b"")
        if size > self._options.window_size:
            if row:
                return self._row_arena.append(bytes(self._scratch_view(offset, size)))
            return self._scratch_view(offset, size)
        base = (offset // self._options.window_size) * self._options.window_size
        window = self._windows.get(base)
        if window is None or offset + size > window.base + window.length:
            if window is not None and window.row_pinned:
                if row:
                    return self._row_arena.append(bytes(self._scratch_view(offset, size)))
                return self._scratch_view(offset, size)
            if len(self._windows) >= self._options.max_windows and self._all_windows_row_pinned():
                if row:
                    return self._row_arena.append(bytes(self._scratch_view(offset, size)))
                return self._scratch_view(offset, size)
            window = self._read_window(base, offset, size)
        else:
            self._windows.move_to_end(base)
        if row:
            window.row_pinned = True
        start = offset - window.base
        return window.view[start:start + size]

    def _read_window(self, base, offset, size):
        existing = self._windows.get(base)
        if existing is not None:
            if existing.row_pinned:
                raise RuntimeError("cannot replace row-pinned read-at window")
            self._close_window(base)
            self._eviction_count += 1
        else:
            self._evict_to_budget(extra_needed=1)
        length = min(self._options.window_size, self._visible_size - base)
        if offset + size > base + length:
            length = offset + size - base
        data = read_at(self._fd, length, base)
        if len(data) != length:
            raise ValueError("short read")
        window = _ReadAtWindow(base, data)
        self._windows[base] = window
        self._window_miss_count += 1
        self._evict_to_budget()
        return window

    def _all_windows_row_pinned(self):
        return bool(self._windows) and all(window.row_pinned for window in self._windows.values())

    def _evict_to_budget(self, extra_needed=0):
        while len(self._windows) + extra_needed > self._options.max_windows:
            for base, window in list(self._windows.items()):
                if not window.row_pinned:
                    self._windows.pop(base, None)
                    self._eviction_count += 1
                    break
            else:
                return

    def _close_window(self, base):
        self._windows.pop(base, None)

    def _read_buffer_bytes(self):
        return sum(window.length for window in self._windows.values())


class _RowArena:
    def __init__(self, limit_bytes, segment_bytes):
        self.limit_bytes = int(limit_bytes)
        self.segment_bytes = int(segment_bytes)
        self.segments = []
        self.current_bytes = 0
        self.peak_bytes = 0
        self._used_in_current = 0

    def append(self, data):
        data = bytes(data)
        size = len(data)
        if self.current_bytes + size > self.limit_bytes:
            raise RuntimeError("row arena limit exceeded")
        if not self.segments or self._remaining_current() < size:
            segment_size = max(self.segment_bytes, size)
            self.segments.append(bytearray(segment_size))
            self._used_in_current = 0
        segment = self.segments[-1]
        start = self._used_in_current
        segment[start:start + size] = data
        self._used_in_current += size
        self.current_bytes += size
        self.peak_bytes = max(self.peak_bytes, self.current_bytes)
        return memoryview(segment)[start:start + size]

    def clear(self):
        self.segments = []
        self.current_bytes = 0
        self._used_in_current = 0

    def _remaining_current(self):
        if not self.segments:
            return 0
        return len(self.segments[-1]) - self._used_in_current
