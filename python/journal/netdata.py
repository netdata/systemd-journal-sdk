#!/usr/bin/env python3
# SPDX-License-Identifier: MIT-0
"""Netdata function surface for the systemd journal SDK (Python port).

This module is the Python equivalent of `rust/src/journal/src/netdata.rs`.
It provides:

- The seven `NETDATA_SOURCE_TYPE_*` constants (bit flags).
- The 16 accepted request parameter names.
- `SYSTEMD_DEFAULT_VIEW_KEYS` (18 names) and `SYSTEMD_DEFAULT_FACETS`
  (60 names) copied EXACTLY from the Rust source (order preserved).
- `NetdataFunctionConfig` dataclass and the `systemd_journal()`
  factory.
- `DisplayScope`, `DisplayContext`, and the `NetdataFunctionProfile`
  base class (default methods: utf8-lossy, facet_option_name,
  row_options with PRIORITY -> severity mapping).
- `systemd_field_display_value(...)` transformation family: PRIORITY,
  SYSLOG_FACILITY, ERRNO, MESSAGE_ID (scope-aware), _BOOT_ID (with
  context offset lookups), _UID / _SYSTEMD_OWNER_UID / OBJECT_*_UID /
  _AUDIT_LOGINUID and their OBJECT_ variants, _GID / OBJECT_GID,
  _CAP_EFFECTIVE, _SOURCE_REALTIME_TIMESTAMP, and a default
  utf8-lossy fallback.
- The two concrete profiles:
    * `SystemdJournalProfile` (no host uid/gid resolution)
    * `SystemdJournalPluginProfile` (host uid/gid resolution,
      cached in `DisplayContext`)

This is chunk 2a of SOW-0104: only the foundation. Request parsing,
source discovery, the function wrapper, response envelope, run options,
progress, and state are added in later chunks.

Stdlib-only. No `journal.*` runtime imports.
"""

from __future__ import annotations

import grp
import mmap
import os
import pathlib
import pwd
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Mapping, Optional, Sequence, Union


# ---------------------------------------------------------------------------
# Source-type bit flags
# ---------------------------------------------------------------------------

NETDATA_SOURCE_TYPE_ALL = 1 << 0
NETDATA_SOURCE_TYPE_LOCAL_ALL = 1 << 1
NETDATA_SOURCE_TYPE_REMOTE_ALL = 1 << 2
NETDATA_SOURCE_TYPE_LOCAL_SYSTEM = 1 << 3
NETDATA_SOURCE_TYPE_LOCAL_USER = 1 << 4
NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE = 1 << 5
NETDATA_SOURCE_TYPE_LOCAL_OTHER = 1 << 6


# ---------------------------------------------------------------------------
# Accepted request parameter names (request body keys).
# ---------------------------------------------------------------------------

NETDATA_ACCEPTED_PARAMS: List[str] = [
    "info",
    "__logs_sources",
    "after",
    "before",
    "anchor",
    "direction",
    "last",
    "query",
    "facets",
    "histogram",
    "if_modified_since",
    "data_only",
    "delta",
    "tail",
    "sampling",
    "slice",
]


# ---------------------------------------------------------------------------
# Default view keys (18) and facets (60) - COPIED EXACTLY from
# rust/src/journal/src/netdata.rs L73-157. Order is significant:
# it drives UI column order.
# ---------------------------------------------------------------------------

SYSTEMD_DEFAULT_VIEW_KEYS: List[str] = [
    "_HOSTNAME",
    "ND_JOURNAL_PROCESS",
    "MESSAGE",
    "PRIORITY",
    "SYSLOG_FACILITY",
    "ERRNO",
    "ND_JOURNAL_FILE",
    "SYSLOG_IDENTIFIER",
    "UNIT",
    "USER_UNIT",
    "MESSAGE_ID",
    "_BOOT_ID",
    "_SYSTEMD_OWNER_UID",
    "_UID",
    "OBJECT_SYSTEMD_OWNER_UID",
    "OBJECT_UID",
    "_GID",
    "OBJECT_GID",
    "_CAP_EFFECTIVE",
    "_AUDIT_LOGINUID",
    "OBJECT_AUDIT_LOGINUID",
    "_SOURCE_REALTIME_TIMESTAMP",
]

SYSTEMD_DEFAULT_FACETS: List[str] = [
    "_HOSTNAME",
    "PRIORITY",
    "SYSLOG_FACILITY",
    "ERRNO",
    "SYSLOG_IDENTIFIER",
    "UNIT",
    "USER_UNIT",
    "MESSAGE_ID",
    "_BOOT_ID",
    "_SYSTEMD_OWNER_UID",
    "_UID",
    "OBJECT_SYSTEMD_OWNER_UID",
    "OBJECT_UID",
    "_GID",
    "OBJECT_GID",
    "_AUDIT_LOGINUID",
    "OBJECT_AUDIT_LOGINUID",
    "CODE_FILE",
    "_SYSTEMD_UNIT",
    "_SYSTEMD_USER_SLICE",
    "CODE_FUNC",
    "_TRANSPORT",
    "_COMM",
    "_RUNTIME_SCOPE",
    "_MACHINE_ID",
    "_SYSTEMD_SLICE",
    "UNIT_RESULT",
    "_SYSTEMD_CGROUP",
    "_EXE",
    "_SYSTEMD_USER_UNIT",
    "_SYSTEMD_SESSION",
    "COREDUMP_CGROUP",
    "COREDUMP_USER_UNIT",
    "COREDUMP_UNIT",
    "COREDUMP_SIGNAL_NAME",
    "COREDUMP_COMM",
    "_UDEV_DEVNODE",
    "_KERNEL_SUBSYSTEM",
    "OBJECT_EXE",
    "OBJECT_SYSTEMD_CGROUP",
    "OBJECT_COMM",
    "OBJECT_SYSTEMD_UNIT",
    "OBJECT_SYSTEMD_USER_UNIT",
    "_SELINUX_CONTEXT",
    "_NAMESPACE",
    "OBJECT_SYSTEMD_SESSION",
    "CONTAINER_ID",
    "CONTAINER_NAME",
    "CONTAINER_TAG",
    "IMAGE_NAME",
    "ND_NIDL_NODE",
    "ND_NIDL_CONTEXT",
    "ND_LOG_SOURCE",
    "ND_ALERT_NAME",
    "ND_ALERT_CLASS",
    "ND_ALERT_COMPONENT",
    "ND_ALERT_TYPE",
    "ND_ALERT_STATUS",
]


# ---------------------------------------------------------------------------
# Internal behaviour constants (mirror of Rust L20-37, exposed only
# when a later chunk needs them; defined here for parity).
# ---------------------------------------------------------------------------

DEFAULT_FUNCTION_NAME = "systemd-journal"
DEFAULT_SOURCE_SELECTOR_NAME = "Journal Sources"
DEFAULT_SOURCE_SELECTOR_HELP = "Select the logs source to query"
DEFAULT_ITEMS_TO_RETURN = 200
DEFAULT_TIME_WINDOW_SECONDS = 3600
DEFAULT_ITEMS_SAMPLING = 1_000_000
DEFAULT_HISTOGRAM_BUCKETS = 150
EFFECTIVELY_DISABLED_TIMEOUT_SECONDS = 100 * 365 * 24 * 60 * 60
NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC = 5_000_000
NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC = 2 * 60 * 1_000_000
NETDATA_FACET_MAX_VALUE_LENGTH = 8192
NETDATA_MAX_DIRECTORY_SCAN_DEPTH = 64
NETDATA_MAX_DIRECTORY_SCAN_COUNT = 8192

DATA_ONLY_CHECK_EVERY_ROWS = 128


# ---------------------------------------------------------------------------
# State hook (mirror of Rust L284-300)
# ---------------------------------------------------------------------------


@dataclass
class NetdataJournalFileMetadata:
    """Cached per-file metadata consulted by the Netdata function.

    Mirrors `NetdataJournalFileMetadata` (Rust L284-292). All fields
    are optional; absent fields fall back to per-file detection.
    """

    source_type: "Optional[int]" = None
    source_name: "Optional[str]" = None
    file_last_modified_usec: "Optional[int]" = None
    msg_first_realtime_usec: "Optional[int]" = None
    msg_last_realtime_usec: "Optional[int]" = None
    journal_vs_realtime_delta_usec: "Optional[int]" = None


class NetdataFunctionState:
    """Caller-supplied state hook for file metadata caching.

    Mirrors the Rust trait `NetdataFunctionState` (L294-300). The
    default `file_metadata` returns None and `update_file_*` is a
    no-op, so callers can subclass and override only what they need.
    """

    def file_metadata(self, path: str) -> "Optional[NetdataJournalFileMetadata]":
        return None

    def update_file_journal_vs_realtime_delta_usec(self, path: str, delta_usec: int) -> None:
        return None


# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------


@dataclass
class NetdataFunctionConfig:
    """Configuration for a Netdata function instance.

    Mirrors `rust/src/journal/src/netdata.rs::NetdataFunctionConfig`.
    The factory `NetdataFunctionConfig.systemd_journal()` returns the
    defaults that match the Rust SDK. The default constructor is
    equivalent to `systemd_journal()`.
    """

    function_name: str = DEFAULT_FUNCTION_NAME
    source_selector_name: str = DEFAULT_SOURCE_SELECTOR_NAME
    source_selector_help: str = DEFAULT_SOURCE_SELECTOR_HELP
    default_facets: List[str] = field(default_factory=lambda: list(SYSTEMD_DEFAULT_FACETS))
    default_view_keys: List[str] = field(default_factory=lambda: list(SYSTEMD_DEFAULT_VIEW_KEYS))
    default_histogram: Optional[str] = "PRIORITY"
    # `reader_options` and `explorer_strategy` are filled in by the
    # later explorer/function chunks; left as placeholders here so this
    # dataclass can stand on its own.
    #
    # chunk 2b: replace with `ReaderOptions.snapshot()` and the
    # `ExplorerStrategy` enum once the explorer surface is exported.
    reader_options: Any = None
    explorer_strategy: Any = None

    @classmethod
    def systemd_journal(cls) -> "NetdataFunctionConfig":
        return cls()

    # chunk 2b: add the backfill logic for empty selector
    # name/help that Rust's `NetdataJournalFunction::new` performs.

    def backfill_defaults(self) -> "NetdataFunctionConfig":
        """Backfill empty `source_selector_name` / `source_selector_help`.

        Mirrors `NetdataJournalFunction::new` (L361-367): if either
        selector string is empty, restore the default. Returns `self`
        for chaining.
        """
        if not self.source_selector_name:
            self.source_selector_name = DEFAULT_SOURCE_SELECTOR_NAME
        if not self.source_selector_help:
            self.source_selector_help = DEFAULT_SOURCE_SELECTOR_HELP
        return self


# ---------------------------------------------------------------------------
# Display scope / context
# ---------------------------------------------------------------------------


class DisplayScope(Enum):
    """Where a transformed field value is going to be consumed.

    Mirrors `rust/src/journal/src/netdata.rs::DisplayScope` (L205-210).
    """

    Data = "data"
    Facet = "facet"
    Histogram = "histogram"


class DisplayContext:
    """Reusable per-function display state.

    Mirrors `rust/src/journal/src/netdata.rs::DisplayContext`
    (L198-203). Three caches are private and may be reused across
    requests so that uid/gid name lookups and boot-id timestamps are
    computed at most once per distinct value within a function
    lifetime.
    """

    __slots__ = ("_boot_first_realtime", "_uid_cache", "_gid_cache")

    def __init__(self) -> None:
        # Boot-id (bytes) -> first-realtime usec.
        self._boot_first_realtime: Dict[bytes, int] = {}
        # uid (str) -> display string.
        self._uid_cache: Dict[str, str] = {}
        # gid (str) -> display string.
        self._gid_cache: Dict[str, str] = {}

    # The next three properties exist for parity with the Rust private
    # fields; they are read-only views and not part of the public API.
    @property
    def boot_first_realtime(self) -> Mapping[bytes, int]:
        return self._boot_first_realtime

    @property
    def uid_display_cache(self) -> Mapping[str, str]:
        return self._uid_cache

    @property
    def gid_display_cache(self) -> Mapping[str, str]:
        return self._gid_cache

    def register_boot_first_realtime(self, boot_id: bytes, realtime_usec: int) -> None:
        """Record the first `_SOURCE_REALTIME_TIMESTAMP` seen for `boot_id`.

        The full function pipeline (chunk 2b+) populates this while
        scanning files so that subsequent `_BOOT_ID` displays can show
        the boot's first message timestamp. Kept here so the
        `systemd_field_display_value` family has a public way to seed
        the cache.
        """
        self._boot_first_realtime[boot_id] = int(realtime_usec)


# ---------------------------------------------------------------------------
# Profile base class + the two concrete profiles
# ---------------------------------------------------------------------------


class NetdataFunctionProfile:
    """Base class for per-field display rules.

    Mirrors the Rust trait `NetdataFunctionProfile` (L212-236). The
    default methods are: a utf8-lossy fallback for `field_display_value`,
    a `facet_option_name` that reuses `field_display_value` and
    stringifies non-string values, and a `row_options` that maps
    `PRIORITY` to a Netdata row severity string.
    """

    # chunk 2b: profiles may want to override `field_display_value`.
    def field_display_value(
        self,
        context: DisplayContext,
        scope: DisplayScope,
        field: str,
        value: Union[bytes, bytearray, memoryview],
    ) -> str:
        return _bytes_to_text(value)

    def facet_option_name(
        self,
        context: DisplayContext,
        field: str,
        raw_value: Union[bytes, bytearray, memoryview],
    ) -> str:
        rendered = self.field_display_value(context, DisplayScope.Facet, field, raw_value)
        if isinstance(rendered, str):
            return rendered
        return str(rendered)

    def row_options(self, fields: Mapping[str, Sequence[Union[bytes, bytearray, memoryview]]]) -> Dict[str, str]:
        priority_values = fields.get("PRIORITY") or []
        if priority_values:
            return {"severity": _priority_to_row_severity(priority_values[0])}
        return {"severity": "normal"}


class SystemdJournalProfile(NetdataFunctionProfile):
    """Standard systemd journal profile.

    Mirrors `SystemdJournalProfile` (L238-253). Does NOT resolve uid
    or gid names against the host database - it returns the raw value.
    """

    def field_display_value(
        self,
        context: DisplayContext,
        scope: DisplayScope,
        field: str,
        value: Union[bytes, bytearray, memoryview],
    ) -> str:
        return _systemd_field_display_value(context, scope, field, value, resolve_user_group_names=False)


class SystemdJournalPluginProfile(NetdataFunctionProfile):
    """Plugin-compatible systemd journal profile.

    Mirrors `SystemdJournalPluginProfile` (L241-265). Resolves uid
    and gid names against the system user database (caching results
    in `DisplayContext`).
    """

    def field_display_value(
        self,
        context: DisplayContext,
        scope: DisplayScope,
        field: str,
        value: Union[bytes, bytearray, memoryview],
    ) -> str:
        return _systemd_field_display_value(context, scope, field, value, resolve_user_group_names=True)


# ---------------------------------------------------------------------------
# Helpers (private, but unit-tested)
# ---------------------------------------------------------------------------


def _bytes_to_text(value: Union[bytes, bytearray, memoryview]) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value).decode("utf-8", errors="replace")
    return str(value)


_UID_FIELDS = frozenset({
    "_UID",
    "_SYSTEMD_OWNER_UID",
    "OBJECT_SYSTEMD_OWNER_UID",
    "OBJECT_UID",
    "_AUDIT_LOGINUID",
    "OBJECT_AUDIT_LOGINUID",
})

_GID_FIELDS = frozenset({"_GID", "OBJECT_GID"})


def _try_int(raw: str) -> Optional[int]:
    """Parse `raw` as an integer. Returns None on failure.

    Mirrors the `parse::<u32>()` / `parse::<u64>()` patterns used in
    the Rust helpers. We only need non-negative values so we accept
    anything int() accepts and reject negatives.
    """
    try:
        parsed = int(raw)
    except (TypeError, ValueError):
        return None
    if parsed < 0:
        return None
    return parsed


def _priority_name(raw: str) -> Optional[str]:
    """Map a numeric priority to its short name.

    Mirrors `rust/src/journal/src/netdata.rs::priority_name`
    (L4053-4065). Returns None if the value is not a 0..=7 integer.
    """
    parsed = _try_int(raw)
    if parsed is None:
        return None
    return {
        0: "panic",
        1: "alert",
        2: "critical",
        3: "error",
        4: "warning",
        5: "notice",
        6: "info",
        7: "debug",
    }.get(parsed)


def _priority_to_row_severity(raw: Union[bytes, bytearray, memoryview, str]) -> str:
    """Map a raw PRIORITY value to a Netdata row severity.

    Mirrors `priority_to_row_severity` (L4085-4094):
        <=3   -> "critical"
        4     -> "warning"
        5     -> "notice"
        >=7   -> "debug"
        else  -> "normal"
    """
    text = _bytes_to_text(raw)
    parsed = _try_int(text)
    if parsed is None:
        return "normal"
    if parsed <= 3:
        return "critical"
    if parsed == 4:
        return "warning"
    if parsed == 5:
        return "notice"
    if parsed >= 7:
        return "debug"
    return "normal"


# SYSLOG_FACILITY lookup, copy of L4096-4120.
_SYSLOG_FACILITY_NAMES = {
    0: "kern",
    1: "user",
    2: "mail",
    3: "daemon",
    4: "auth",
    5: "syslog",
    6: "lpr",
    7: "news",
    8: "uucp",
    9: "cron",
    10: "authpriv",
    11: "ftp",
    16: "local0",
    17: "local1",
    18: "local2",
    19: "local3",
    20: "local4",
    21: "local5",
    22: "local6",
    23: "local7",
}


def _syslog_facility_name(raw: str) -> Optional[str]:
    parsed = _try_int(raw)
    if parsed is None:
        return None
    return _SYSLOG_FACILITY_NAMES.get(parsed)


# ERRNO lookup, copy of L4122-4254.
_ERRNO_NAMES = {
    1: "EPERM", 2: "ENOENT", 3: "ESRCH", 4: "EINTR", 5: "EIO",
    6: "ENXIO", 7: "E2BIG", 8: "ENOEXEC", 9: "EBADF", 10: "ECHILD",
    11: "EAGAIN", 12: "ENOMEM", 13: "EACCES", 14: "EFAULT", 15: "ENOTBLK",
    16: "EBUSY", 17: "EEXIST", 18: "EXDEV", 19: "ENODEV", 20: "ENOTDIR",
    21: "EISDIR", 22: "EINVAL", 23: "ENFILE", 24: "EMFILE", 25: "ENOTTY",
    26: "ETXTBSY", 27: "EFBIG", 28: "ENOSPC", 29: "ESPIPE", 30: "EROFS",
    31: "EMLINK", 32: "EPIPE", 33: "EDOM", 34: "ERANGE", 35: "EDEADLK",
    36: "ENAMETOOLONG", 37: "ENOLCK", 38: "ENOSYS", 39: "ENOTEMPTY",
    40: "ELOOP", 42: "ENOMSG", 43: "EIDRM", 44: "ECHRNG", 45: "EL2NSYNC",
    46: "EL3HLT", 47: "EL3RST", 48: "ELNRNG", 49: "EUNATCH", 50: "ENOCSI",
    51: "EL2HLT", 52: "EBADE", 53: "EBADR", 54: "EXFULL", 55: "ENOANO",
    56: "EBADRQC", 57: "EBADSLT", 59: "EBFONT", 60: "ENOSTR", 61: "ENODATA",
    62: "ETIME", 63: "ENOSR", 64: "ENONET", 65: "ENOPKG", 66: "EREMOTE",
    67: "ENOLINK", 68: "EADV", 69: "ESRMNT", 70: "ECOMM", 71: "EPROTO",
    72: "EMULTIHOP", 73: "EDOTDOT", 74: "EBADMSG", 75: "EOVERFLOW",
    76: "ENOTUNIQ", 77: "EBADFD", 78: "EREMCHG", 79: "ELIBACC",
    80: "ELIBBAD", 81: "ELIBSCN", 82: "ELIBMAX", 83: "ELIBEXEC",
    84: "EILSEQ", 85: "ERESTART", 86: "ESTRPIPE", 87: "EUSERS",
    88: "ENOTSOCK", 89: "EDESTADDRREQ", 90: "EMSGSIZE", 91: "EPROTOTYPE",
    92: "ENOPROTOOPT", 93: "EPROTONOSUPPORT", 94: "ESOCKTNOSUPPORT",
    95: "ENOTSUP", 96: "EPFNOSUPPORT", 97: "EAFNOSUPPORT",
    98: "EADDRINUSE", 99: "EADDRNOTAVAIL", 100: "ENETDOWN",
    101: "ENETUNREACH", 102: "ENETRESET", 103: "ECONNABORTED",
    104: "ECONNRESET", 105: "ENOBUFS", 106: "EISCONN", 107: "ENOTCONN",
    108: "ESHUTDOWN", 109: "ETOOMANYREFS", 110: "ETIMEDOUT",
    111: "ECONNREFUSED", 112: "EHOSTDOWN", 113: "EHOSTUNREACH",
    114: "EALREADY", 115: "EINPROGRESS", 116: "ESTALE", 117: "EUCLEAN",
    118: "ENOTNAM", 119: "ENAVAIL", 120: "EISNAM", 121: "EREMOTEIO",
    122: "EDQUOT", 123: "ENOMEDIUM", 124: "EMEDIUMTYPE", 125: "ECANCELED",
    126: "ENOKEY", 127: "EKEYEXPIRED", 128: "EKEYREVOKED",
    129: "EKEYREJECTED", 130: "EOWNERDEAD", 131: "ENOTRECOVERABLE",
    132: "ERFKILL", 133: "EHWPOISON",
}


def _errno_name(raw: str) -> Optional[str]:
    parsed = _try_int(raw)
    if parsed is None:
        return None
    name = _ERRNO_NAMES.get(parsed)
    if name is None:
        return None
    return f"{parsed} ({name})"


# Capability names, copy of L4274-4316.
_CAPABILITIES = [
    "CHOWN", "DAC_OVERRIDE", "DAC_READ_SEARCH", "FOWNER", "FSETID",
    "KILL", "SETGID", "SETUID", "SETPCAP", "LINUX_IMMUTABLE",
    "NET_BIND_SERVICE", "NET_BROADCAST", "NET_ADMIN", "NET_RAW",
    "IPC_LOCK", "IPC_OWNER", "SYS_MODULE", "SYS_RAWIO", "SYS_CHROOT",
    "SYS_PTRACE", "SYS_PACCT", "SYS_ADMIN", "SYS_BOOT", "SYS_NICE",
    "SYS_RESOURCE", "SYS_TIME", "SYS_TTY_CONFIG", "MKNOD", "LEASE",
    "AUDIT_WRITE", "AUDIT_CONTROL", "SETFCAP", "MAC_OVERRIDE",
    "MAC_ADMIN", "SYSLOG", "WAKE_ALARM", "BLOCK_SUSPEND", "AUDIT_READ",
    "PERFMON", "BPF", "CHECKPOINT_RESTORE",
]


def _cap_effective_display(raw: str) -> str:
    """Decode a hex `_CAP_EFFECTIVE` bitmask into "raw (CAP_A | CAP_B)".

    Mirrors `cap_effective_display` (L4264-4327). If the value is not
    a non-zero hex integer (or parses to 0), the raw value is
    returned unchanged.
    """
    if not raw:
        return raw
    first = raw[:1]
    if not (first.isascii() and first.isdigit()):
        return raw
    try:
        value = int(raw, 16)
    except ValueError:
        return raw
    if value == 0:
        return raw
    names = [name for index, name in enumerate(_CAPABILITIES) if (value >> index) & 1]
    if not names:
        return raw
    return f"{raw} ({' | '.join(names)})"


# Message-ID names, copy of L4477-4791. All entries are looked up
# at runtime by exact hex string match.
_MESSAGE_ID_NAMES = {
    "f77379a8490b408bbe5f6940505a777b": "Journal started",
    "d93fb3c9c24d451a97cea615ce59c00b": "Journal stopped",
    "a596d6fe7bfa4994828e72309e95d61e": "Journal messages suppressed",
    "e9bf28e6e834481bb6f48f548ad13606": "Journal messages missed",
    "ec387f577b844b8fa948f33cad9a75e6": "Journal disk space usage",
    "fc2e22bc6ee647b6b90729ab34a250b1": "Coredump",
    "5aadd8e954dc4b1a8c954d63fd9e1137": "Coredump truncated",
    "1f4e0a44a88649939aaea34fc6da8c95": "Backtrace",
    "8d45620c1a4348dbb17410da57c60c66": "User Session created",
    "3354939424b4456d9802ca8333ed424a": "User Session terminated",
    "fcbefc5da23d428093f97c82a9290f7b": "Seat started",
    "e7852bfe46784ed0accde04bc864c2d5": "Seat removed",
    "24d8d4452573402496068381a6312df2": "VM or container started",
    "58432bd3bace477cb514b56381b8a758": "VM or container stopped",
    "c7a787079b354eaaa9e77b371893cd27": "Time change",
    "45f82f4aef7a4bbf942ce861d1f20990": "Timezone change",
    "50876a9db00f4c40bde1a2ad381c3a1b": "System configuration issues",
    "b07a249cd024414a82dd00cd181378ff": "System start-up completed",
    "eed00a68ffd84e31882105fd973abdd1": "User start-up completed",
    "6bbd95ee977941e497c48be27c254128": "Sleep start",
    "8811e6df2a8e40f58a94cea26f8ebf14": "Sleep stop",
    "98268866d1d54a499c4e98921d93bc40": "System shutdown initiated",
    "c14aaf76ec284a5fa1f105f88dfb061c": "System factory reset initiated",
    "d9ec5e95e4b646aaaea2fd05214edbda": "Container init crashed",
    "3ed0163e868a4417ab8b9e210407a96c": "System reboot failed after crash",
    "645c735537634ae0a32b15a7c6cba7d4": "Init execution froze",
    "5addb3a06a734d3396b794bf98fb2d01": "Init crashed no coredump",
    "5c9e98de4ab94c6a9d04d0ad793bd903": "Init crashed no fork",
    "5e6f1f5e4db64a0eaee3368249d20b94": "Init crashed unknown signal",
    "83f84b35ee264f74a3896a9717af34cb": "Init crashed systemd signal",
    "3a73a98baf5b4b199929e3226c0be783": "Init crashed process signal",
    "2ed18d4f78ca47f0a9bc25271c26adb4": "Init crashed waitpid failed",
    "56b1cd96f24246c5b607666fda952356": "Init crashed coredump failed",
    "4ac7566d4d7548f4981f629a28f0f829": "Init crashed coredump",
    "38e8b1e039ad469291b18b44c553a5b7": "Crash shell failed to fork",
    "872729b47dbe473eb768ccecd477beda": "Crash shell failed to execute",
    "658a67adc1c940b3b3316e7e8628834a": "Selinux failed",
    "e6f456bd92004d9580160b2207555186": "Battery low warning",
    "267437d33fdd41099ad76221cc24a335": "Battery low powering off",
    "79e05b67bc4545d1922fe47107ee60c5": "Manager mainloop failed",
    "dbb136b10ef4457ba47a795d62f108c9": "Manager no xdgdir path",
    "ed158c2df8884fa584eead2d902c1032": "Init failed to drop capability bounding set of usermode",
    "42695b500df048298bee37159caa9f2e": "Init failed to drop capability bounding set",
    "bfc2430724ab44499735b4f94cca9295": "User manager can't disable new privileges",
    "59288af523be43a28d494e41e26e4510": "Manager failed to start default target",
    "689b4fcc97b4486ea5da92db69c9e314": "Manager failed to isolate default target",
    "5ed836f1766f4a8a9fc5da45aae23b29": "Manager failed to collect passed file descriptors",
    "6a40fbfbd2ba4b8db02fb40c9cd090d7": "Init failed to fix up environment variables",
    "0e54470984ac419689743d957a119e2e": "Manager failed to allocate",
    "d67fa9f847aa4b048a2ae33535331adb": "Manager failed to write Smack",
    "af55a6f75b544431b72649f36ff6d62c": "System shutdown critical error",
    "d18e0339efb24a068d9c1060221048c2": "Init failed to fork off valgrind",
    "7d4958e842da4a758f6c1cdc7b36dcc5": "Unit starting",
    "39f53479d3a045ac8e11786248231fbf": "Unit started",
    "be02cf6855d2428ba40df7e9d022f03d": "Unit failed",
    "de5b426a63be47a7b6ac3eaac82e2f6f": "Unit stopping",
    "9d1aaa27d60140bd96365438aad20286": "Unit stopped",
    "d34d037fff1847e6ae669a370e694725": "Unit reloading",
    "7b05ebc668384222baa8881179cfda54": "Unit reloaded",
    "5eb03494b6584870a536b337290809b3": "Unit restart scheduled",
    "ae8f7b866b0347b9af31fe1c80b127c0": "Unit resources",
    "7ad2d189f7e94e70a38c781354912448": "Unit success",
    "0e4284a0caca4bfc81c0bb6786972673": "Unit skipped",
    "d9b373ed55a64feb8242e02dbe79a49c": "Unit failure result",
    "641257651c1b4ec9a8624d7a40a9e1e7": "Process execution failed",
    "98e322203f7a4ed290d09fe03c09fe15": "Unit process exited",
    "0027229ca0644181a76c4e92458afa2e": "Syslog forward missed",
    "1dee0369c7fc4736b7099b38ecb46ee7": "Mount point is not empty",
    "d989611b15e44c9dbf31e3c81256e4ed": "Unit oomd kill",
    "fe6faa94e7774663a0da52717891d8ef": "Unit out of memory",
    "b72ea4a2881545a0b50e200e55b9b06f": "Lid opened",
    "b72ea4a2881545a0b50e200e55b9b070": "Lid closed",
    "f5f416b862074b28927a48c3ba7d51ff": "System docked",
    "51e171bd585248568110144c517cca53": "System undocked",
    "b72ea4a2881545a0b50e200e55b9b071": "Power key",
    "3e0117101eb243c1b9a50db3494ab10b": "Power key long press",
    "9fa9d2c012134ec385451ffe316f97d0": "Reboot key",
    "f1c59a58c9d943668965c337caec5975": "Reboot key long press",
    "b72ea4a2881545a0b50e200e55b9b072": "Suspend key",
    "bfdaf6d312ab4007bc1fe40a15df78e8": "Suspend key long press",
    "b72ea4a2881545a0b50e200e55b9b073": "Hibernate key",
    "167836df6f7f428e98147227b2dc8945": "Hibernate key long press",
    "c772d24e9a884cbeb9ea12625c306c01": "Invalid configuration",
    "1675d7f172174098b1108bf8c7dc8f5d": "DNSSEC validation failed",
    "4d4408cfd0d144859184d1e65d7c8a65": "DNSSEC trust anchor revoked",
    "36db2dfa5a9045e1bd4af5f93e1cf057": "DNSSEC turned off",
    "b61fdac612e94b9182285b998843061f": "Username unsafe",
    "1b3bb94037f04bbf81028e135a12d293": "Mount point path not suitable",
    "010190138f494e29a0ef6669749531aa": "Device path not suitable",
    "b480325f9c394a7b802c231e51a2752c": "Nobody user unsuitable",
    "1c0454c1bd2241e0ac6fefb4bc631433": "Systemd udev settle deprecated",
    "7c8a41f37b764941a0e1780b1be2f037": "Time initial sync",
    "7db73c8af0d94eeb822ae04323fe6ab6": "Time initial bump",
    "9e7066279dc8403da79ce4b1a69064b2": "Shutdown scheduled",
    "249f6fb9e6e2428c96f3f0875681ffa3": "Shutdown canceled",
    "3f7d5ef3e54f4302b4f0b143bb270cab": "TPM PCR Extended",
    "f9b0be465ad540d0850ad32172d57c21": "Memory Trimmed",
    "a8fa8dacdb1d443e9503b8be367a6adb": "SysV Service Found",
    "187c62eb1e7f463bb530394f52cb090f": "Portable Service attached",
    "76c5c754d628490d8ecba4c9d042112b": "Portable Service detached",
    "9cf56b8baf9546cf9478783a8de42113": "systemd-networkd sysctl changed by foreign process",
    "ad7089f928ac4f7ea00c07457d47ba8a": "SRK into TPM authorization failure",
    "b2bcbaf5edf948e093ce50bbea0e81ec": "Secure Attention Key (SAK) was pressed",
    "7fc63312330b479bb32e598d47cef1a8": "dbus activate no unit",
    "ee9799dab1e24d81b7bee7759a543e1b": "dbus activate masked unit",
    "a0fa58cafd6f4f0c8d003d16ccf9e797": "dbus broker exited",
    "c8c6cde1c488439aba371a664353d9d8": "dbus dirwatch",
    "8af3357071af4153af414daae07d38e7": "dbus dispatch stats",
    "199d4300277f495f84ba4028c984214c": "dbus no sopeergroup",
    "b209c0d9d1764ab38d13b8e00d1784d6": "dbus protocol violation",
    "6fa70fa776044fa28be7a21daf42a108": "dbus receive failed",
    "0ce0fa61d1a9433dabd67417f6b8e535": "dbus service failed open",
    "24dc708d9e6a4226a3efe2033bb744de": "dbus service invalid",
    "f15d2347662d483ea9bcd8aa1a691d28": "dbus sighup",
    "0ce153587afa4095832d233c17a88001": "Gnome SM startup succeeded",
    "10dd2dc188b54a5e98970f56499d1f73": "Gnome SM unrecoverable failure",
    "f3ea493c22934e26811cd62abe8e203a": "Gnome shell started",
    "c7b39b1e006b464599465e105b361485": "Flatpak cache",
    "75ba3deb0af041a9a46272ff85d9e73e": "Flathub pulls",
    "f02bce89a54e4efab3a94a797d26204a": "Flathub pull errors",
    "dd11929c788e48bdbb6276fb5f26b08a": "Boltd starting",
    "1e6061a9fbd44501b3ccc368119f2b69": "Netdata startup",
    "ed4cdb8f1beb4ad3b57cb3cae2d162fa": "Netdata connection from child",
    "6e2e3839067648968b646045dbf28d66": "Netdata connection to parent",
    "9ce0cb58ab8b44df82c4bf1ad9ee22de": "Netdata alert transition",
    "6db0018e83e34320ae2a659d78019fb7": "Netdata alert notification",
    "23e93dfccbf64e11aac858b9410d8a82": "Netdata fatal message",
    "8ddaf5ba33a74078b609250db1e951f3": "Sensor state transition",
    "ec87a56120d5431bace51e2fb8bba243": "Netdata log flood protection",
    "acb33cb95778476baac702eb7e4e151d": "Netdata Cloud connection",
    "d1f59606dd4d41e3b217a0cfcae8e632": "Netdata extreme cardinality",
    "02f47d350af5449197bf7a95b605a468": "Netdata exit reason",
    "4fdf40816c124623a032b7fe73beacb8": "Netdata dynamic configuration",
}


def _message_id_name(raw: str) -> Optional[str]:
    return _MESSAGE_ID_NAMES.get(raw)


def _format_realtime_usec(timestamp: int, micros: bool) -> str:
    """Format a microsecond timestamp as an ISO-8601 UTC string.

    Mirrors `format_realtime_usec` (L4042-4051):
        Seconds: "%Y-%m-%dT%H:%M:%SZ"
        Micros:  "%Y-%m-%dT%H:%M:%S%.6fZ"

    On overflow (negative or out-of-range) the original timestamp is
    returned as a decimal string, matching Rust's
    `unwrap_or_else(|| timestamp.to_string())`.
    """
    if timestamp < 0:
        return str(timestamp)
    seconds, rem_micros = divmod(timestamp, 1_000_000)
    if micros:
        try:
            dt = datetime.fromtimestamp(seconds, tz=timezone.utc)
        except (OverflowError, OSError, ValueError):
            return str(timestamp)
        # Match Rust's %.6f -> 6 fractional digits.
        return f"{dt.strftime('%Y-%m-%dT%H:%M:%S')}.{rem_micros:06d}Z"
    try:
        dt = datetime.fromtimestamp(seconds, tz=timezone.utc)
    except (OverflowError, OSError, ValueError):
        return str(timestamp)
    return dt.strftime("%Y-%m-%dT%H:%M:%SZ")


def _resolve_uid_name(raw: str) -> Optional[str]:
    """Resolve a numeric uid to a login name via the system user database.

    On Unix this calls `pwd.getpwuid` (NSS-backed). On non-Unix it
    returns None, mirroring the Rust `cfg(not(unix))` arm.
    """
    if sys.platform == "win32":
        return None
    parsed = _try_int(raw)
    if parsed is None:
        return None
    try:
        entry = pwd.getpwuid(parsed)
    except (KeyError, OverflowError, ValueError):
        return None
    return entry.pw_name


def _resolve_gid_name(raw: str) -> Optional[str]:
    """Resolve a numeric gid to a group name via the system group database."""
    if sys.platform == "win32":
        return None
    parsed = _try_int(raw)
    if parsed is None:
        return None
    try:
        entry = grp.getgrgid(parsed)
    except (KeyError, OverflowError, ValueError):
        return None
    return entry.gr_name


def _cached_uid_display(context: DisplayContext, raw: str) -> str:
    cached = context._uid_cache.get(raw)
    if cached is not None:
        return cached
    resolved = _resolve_uid_name(raw)
    display = resolved if resolved is not None else raw
    context._uid_cache[raw] = display
    return display


def _cached_gid_display(context: DisplayContext, raw: str) -> str:
    cached = context._gid_cache.get(raw)
    if cached is not None:
        return cached
    resolved = _resolve_gid_name(raw)
    display = resolved if resolved is not None else raw
    context._gid_cache[raw] = display
    return display


# ---------------------------------------------------------------------------
# Field-by-field display rule. Mirrors `systemd_field_display_value`
# (L4329-4389) exactly: same fields, same scope-aware formatting,
# same uid/gid resolution gating.
# ---------------------------------------------------------------------------


def _systemd_field_display_value(
    context: DisplayContext,
    scope: DisplayScope,
    field: str,
    value: Union[bytes, bytearray, memoryview],
    resolve_user_group_names: bool,
) -> str:
    raw = _bytes_to_text(value)
    value_bytes = bytes(value) if isinstance(value, (bytes, bytearray, memoryview)) else raw.encode("utf-8", errors="replace")

    if field == "PRIORITY":
        return _priority_name(raw) or raw

    if field == "SYSLOG_FACILITY":
        return _syslog_facility_name(raw) or raw

    if field == "ERRNO":
        return _errno_name(raw) or raw

    if field == "MESSAGE_ID":
        name = _message_id_name(raw)
        if name is not None:
            if scope == DisplayScope.Data:
                return f"{raw} ({name})"
            return name
        return raw

    if field == "_BOOT_ID":
        ts = context._boot_first_realtime.get(value_bytes)
        if ts is not None:
            formatted = _format_realtime_usec(ts, micros=False)
            if scope == DisplayScope.Data:
                return f"{raw} ({formatted})  "
            return formatted
        return raw

    if field in _UID_FIELDS:
        if resolve_user_group_names:
            return _cached_uid_display(context, raw)
        return raw

    if field in _GID_FIELDS:
        if resolve_user_group_names:
            return _cached_gid_display(context, raw)
        return raw

    if field == "_CAP_EFFECTIVE":
        return _cap_effective_display(raw)

    if field == "_SOURCE_REALTIME_TIMESTAMP":
        parsed = _try_int(raw)
        if parsed is not None and parsed != 0:
            return f"{raw} ({_format_realtime_usec(parsed, micros=True)})"
        return raw

    # Default: utf8-lossy passthrough.
    return raw


# ---------------------------------------------------------------------------
# Chunk 2b: Netdata request handling, source discovery, multi-file
# exploration, and response envelope.
#
# This section is a pure-Python port of the Netdata-layer surface from
# `rust/src/journal/src/netdata.rs`. It implements:
#
# - `NetdataRequest` parsing for the 16 accepted parameters (decoding is
#   total; BEHAVIOR for delta / tail / if_modified_since / data_only is
#   exercised by the `__logs_sources` and `info` responses and the
#   full data-response envelope, and is fully implemented here; later
#   chunks may add data-only short-circuits).
# - `collect_journal_files` directory walk (depth 64, count 8192) and
#   `journal_file_source_type` classification with `local_namespace_source_name`
#   and `journal_file_exact_source_name` for remote/local-namespace splits.
# - A `CombinedResult` accumulator with `merge(path, result, direction, limit)`
#   matching `CombinedResult::merge` semantics: rows appended, facet counts
#   summed, histogram bucketed sums, stats merged with the EXACT aggregate-vs-last
#   mapping documented below, then `sort_and_limit` to direction + limit.
# - The full data-response envelope (info, __logs_sources, data with
#   histogram chart: summary/totals/result/db/view/agents; `view.dimensions.names`
#   is always present, even for an empty window).
#
# Stats merge semantics (per `CombinedResult::merge_stats`, L2220-2297):
#   - rows_examined          : sum
#   - rows_matched           : sum
#   - facet_rows_matched     : sum
#   - rows_returned          : sum then overwritten by sort_and_limit
#   - rows_unsampled         : sum
#   - rows_estimated         : sum
#   - sampling_sampled       : sum
#   - sampling_unsampled     : sum
#   - sampling_estimated     : sum
#   - last_realtime_usec     : max (incoming wins if greater)
#   - max_source_realtime_delta_usec : max (incoming wins if greater)
#   - data_refs_seen         : sum
#   - data_refs_skipped      : sum
#   - data_payloads_loaded   : sum
#   - data_objects_classified: sum
#   - data_cache_hits        : sum
#   - data_cache_misses      : sum
#   - payloads_decompressed  : sum
#   - fts_scans              : sum
#   - facet_updates          : sum
#   - histogram_updates      : sum
#   - returned_row_expansions: sum
#   - early_stop_opportunities: sum
#   - early_stops            : sum
# All fields are aggregate (sum) except the two maxima noted above; nothing
# is "kept from last file" for the stats themselves, but the histogram
# bucket *positions* are taken from the first file and the per-file results
# are summed into them, mirroring `merge_histogram`.
# ---------------------------------------------------------------------------


from collections import deque
from typing import Iterable as _Iterable, Tuple as _Tuple

from .explorer import (
    DEFAULT_HISTOGRAM_TARGET_BUCKETS,
    Direction,
    ExplorerAnchor,
    ExplorerAnchorKind,
    ExplorerControl,
    ExplorerFieldMode,
    ExplorerHistogram,
    ExplorerHistogramBucket,
    ExplorerQuery,
    ExplorerResult,
    ExplorerSampling,
    ExplorerStats,
    ExplorerStopReason,
    ExplorerStrategy,
    _explore_file_reader,
    _explore_files,
    _enumerate_fields_indexed,
    _new_histogram,
)


# ---------------------------------------------------------------------------
# Source discovery + classification
# ---------------------------------------------------------------------------


NETDATA_REMOTE_PATH_FRAGMENT = "/remote/"


def _is_journal_file_name(name: str) -> bool:
    """Mirror Rust `is_journal_file_name` (L3985-3994)."""

    return (
        name.endswith(".journal")
        or name.endswith(".journal~")
        or name.endswith(".journal.zst")
        or name.endswith(".journal~.zst")
    )


def _local_namespace_source_name(path) -> "Optional[str]":
    """Mirror Rust `local_namespace_source_name` (L3442-3446).

    A local namespace directory is named `<machine-id>.<namespace>`;
    the namespace becomes `namespace-<namespace>`. Returns None for
    other layouts.
    """

    parent = path.parent
    if parent is None:
        return None
    parent_name = parent.name
    if "." not in parent_name:
        return None
    _, _, namespace = parent_name.rpartition(".")
    if not namespace:
        return None
    return f"namespace-{namespace}"


def _journal_file_source_type(path) -> int:
    """Mirror Rust `journal_file_source_type` (L3422-3440)."""

    text = str(path)
    if NETDATA_REMOTE_PATH_FRAGMENT in text:
        return (
            NETDATA_SOURCE_TYPE_ALL
            | NETDATA_SOURCE_TYPE_REMOTE_ALL
        )
    namespace = _local_namespace_source_name(path)
    if namespace is not None:
        return (
            NETDATA_SOURCE_TYPE_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE
        )
    name = path.name
    if name.startswith("system"):
        return (
            NETDATA_SOURCE_TYPE_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_SYSTEM
        )
    if name.startswith("user"):
        return (
            NETDATA_SOURCE_TYPE_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_ALL
            | NETDATA_SOURCE_TYPE_LOCAL_USER
        )
    return (
        NETDATA_SOURCE_TYPE_ALL
        | NETDATA_SOURCE_TYPE_LOCAL_ALL
        | NETDATA_SOURCE_TYPE_LOCAL_OTHER
    )


def _journal_file_exact_source_name(path) -> "Optional[str]":
    """Mirror Rust `journal_file_exact_source_name` (L3448-3465)."""

    text = str(path)
    if NETDATA_REMOTE_PATH_FRAGMENT in text:
        name = path.name
        if "@" in name:
            return name.split("@", 1)[0]
        for suffix in (".journal~.zst", ".journal.zst", ".journal~", ".journal"):
            if name.endswith(suffix):
                stripped = name[: -len(suffix)]
                break
        else:
            stripped = name
        if stripped.startswith("remote-"):
            return stripped
        return None
    return _local_namespace_source_name(path)


@dataclass
class JournalFileCollection:
    """Result of `collect_journal_files` (Rust L3841-3846)."""

    files: List[str] = field(default_factory=list)
    skipped: int = 0
    errors: List[str] = field(default_factory=list)


def _canonical(path) -> str:
    """Best-effort canonicalize that falls back to the original string."""

    try:
        return str(pathlib.Path(path).resolve())
    except OSError:
        return str(path)


def _collect_journal_files(directory) -> JournalFileCollection:
    """Mirror Rust `collect_journal_files` (L3848-3895).

    Walks the directory BFS-style, collecting every file whose name
    matches the journal extension set. Depth is bounded by
    `NETDATA_MAX_DIRECTORY_SCAN_DEPTH` (64) and the total visited
    directory count by `NETDATA_MAX_DIRECTORY_SCAN_COUNT` (8192).
    Symlink loop protection and unreadable-subdirectory error
    reporting are preserved.
    """

    from .compress import is_journal_file_name as _ext_is_journal

    p = pathlib.Path(directory)
    if not p.is_dir():
        raise ValueError(f"not a directory: {directory}")
    collection = JournalFileCollection()
    pending: "deque[Tuple[pathlib.Path, int]]" = deque([(p, 0)])
    visited: set = set()
    while pending:
        current, depth = pending.popleft()
        visited_key = _canonical(current)
        if visited_key in visited:
            continue
        if len(visited) >= NETDATA_MAX_DIRECTORY_SCAN_COUNT:
            collection.skipped += 1
            collection.errors.append(
                f"{current}: directory scan limit reached"
            )
            continue
        visited.add(visited_key)
        try:
            entries = list(os.scandir(current))
        except OSError as err:
            if current == p:
                raise
            collection.skipped += 1
            collection.errors.append(f"{current}: {err}")
            continue
        for entry in entries:
            try:
                if entry.is_file(follow_symlinks=False):
                    if _ext_is_journal(entry.name) or _is_journal_file_name(entry.name):
                        collection.files.append(str(entry.path))
                elif entry.is_dir(follow_symlinks=False):
                    if depth < NETDATA_MAX_DIRECTORY_SCAN_DEPTH:
                        pending.append((pathlib.Path(entry.path), depth + 1))
            except OSError:
                continue
    collection.files.sort()
    # Dedupe by canonical path; keep the first occurrence (sort-stable).
    seen: set = set()
    deduped: List[str] = []
    for path_str in collection.files:
        key = _canonical(path_str)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(path_str)
    collection.files = deduped
    return collection


# ---------------------------------------------------------------------------
# Time-window normalization
# ---------------------------------------------------------------------------


API_RELATIVE_TIME_MAX_SECONDS = 3 * 365 * 86_400
NETDATA_MISSING_AFTER_RELATIVE_SECONDS = 600


def _unix_now_seconds() -> int:
    return int(time.time())


def _normalize_timestamp_to_usec_with_rounding(value: int, end_of_second: bool) -> int:
    """Mirror Rust `normalize_timestamp_to_usec_with_rounding` (L3768-3776).

    Values >= 1_000_000_000_000 are treated as already-usec; smaller
    values are seconds and are scaled. When `end_of_second` is set, the
    scaled value is bumped to 999_999 microseconds to make the window
    inclusive at the upper bound.
    """

    if value < 0:
        value = 0
    if value >= 1_000_000_000_000:
        return int(value)
    if end_of_second:
        return int(value) * 1_000_000 + 999_999
    return int(value) * 1_000_000


def _relative_window_to_absolute(now_seconds: int, after: int, before: int) -> "Tuple[int, int]":
    """Mirror Rust `relative_window_to_absolute` (L3658-3690).

    Rust gates the relative branch on the unsigned magnitude only;
    a `0` value still enters the branch and is treated as a relative
    "0 seconds" offset (i.e. `now` for `before`, `-MISSING` for
    `after`). Mirroring that contract keeps the per-endpoint zero
    fallback consistent with the reference implementation.
    """

    if abs(before) <= API_RELATIVE_TIME_MAX_SECONDS:
        if before > 0:
            before = -before
        before = now_seconds + before
    if abs(after) <= API_RELATIVE_TIME_MAX_SECONDS:
        if after > 0:
            after = -after
        if after == 0:
            after = -NETDATA_MISSING_AFTER_RELATIVE_SECONDS
        after = before + after + 1
    if after > before:
        after, before = before, after
    if before > now_seconds:
        delta = before - now_seconds
        before -= delta
        after -= delta
    return after, before


def _normalize_time_window(
    now_seconds: int, after: "Optional[int]", before: "Optional[int]"
) -> "Tuple[Optional[int], Optional[int]]":
    """Mirror Rust `normalize_time_window` (L3624-3656).

    Both `after` and `before` may be in seconds (small) or usec
    (>= 1_000_000_000_000). Returns (after_usec, before_usec) where
    0 is replaced with `now - DEFAULT_TIME_WINDOW_SECONDS` for `after`
    and `now` for `before`. The window is widened to
    `DEFAULT_TIME_WINDOW_SECONDS` if both endpoints are equal.
    """

    a = int(after) if after is not None else 0
    b = int(before) if before is not None else 0
    if a == 0 and b == 0:
        b = now_seconds
        a = b - DEFAULT_TIME_WINDOW_SECONDS
    else:
        a, b = _relative_window_to_absolute(now_seconds, a, b)
    if a > b:
        a, b = b, a
    if a == b:
        a = b - DEFAULT_TIME_WINDOW_SECONDS
    return (
        _normalize_timestamp_to_usec_with_rounding(max(a, 0), False),
        _normalize_timestamp_to_usec_with_rounding(max(b, 0), True),
    )


def _tail_after_realtime_bound(
    after_realtime_usec: "Optional[int]", anchor: "ExplorerAnchor"
) -> "Optional[int]":
    """Mirror Rust `tail_after_realtime_bound` (L3504-3517).

    For a realtime tail anchor, the post-anchor bound is
    `anchor + 1` (exclusive of the anchor itself). When the request
    has its own `after`, take the max; when not, use `anchor + 1`.
    """

    if anchor.kind != ExplorerAnchorKind.REALTIME:
        return after_realtime_usec
    tail_after = int(anchor.realtime_usec) + 1
    if after_realtime_usec is None:
        return tail_after
    return max(int(after_realtime_usec), tail_after)


def _before_realtime_bound_excluding_anchor(
    before_realtime_usec: "Optional[int]", anchor: "ExplorerAnchor"
) -> "Optional[int]":
    """Mirror Rust `before_realtime_bound_excluding_anchor` (L3519-3532).

    For a data-only backward anchor, the bound is `anchor - 1`
    (exclusive). When the request has its own `before`, take the min.
    """

    if anchor.kind != ExplorerAnchorKind.REALTIME:
        return before_realtime_usec
    before_anchor = max(0, int(anchor.realtime_usec) - 1)
    if before_realtime_usec is None:
        return before_anchor
    return min(int(before_realtime_usec), before_anchor)


def _fill_sampling_from_header(
    sampling: "ExplorerSampling", header: Mapping[str, Any]
) -> None:
    """Populate the file-order fields of an `ExplorerSampling` from a
    parsed file header dict (Rust L1552-1580).
    """

    if not header:
        return
    head_realtime = int(header.get("head_entry_realtime", 0) or 0)
    tail_realtime = int(header.get("tail_entry_realtime", 0) or 0)
    head_seqnum = int(header.get("head_entry_seqnum", 0) or 0)
    tail_seqnum = int(header.get("tail_entry_seqnum", 0) or 0)
    sampling.file_head_realtime_usec = head_realtime
    sampling.file_tail_realtime_usec = tail_realtime
    sampling.file_head_seqnum = head_seqnum
    sampling.file_tail_seqnum = tail_seqnum
    if head_seqnum != 0 and tail_seqnum != 0:
        span = tail_seqnum - head_seqnum + 1
        if span > 0:
            sampling.file_entries = int(span)
            return
    sampling.file_entries = int(header.get("n_entries", 0) or 0)


def _normalize_journal_vs_realtime_delta_usec(delta_usec: int) -> int:
    """Mirror Rust `normalize_journal_vs_realtime_delta_usec` (L2891-2895).

    Clamp to `[DEFAULT, MAX]`. A `0` input becomes the default.
    """

    if delta_usec <= 0:
        return NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC
    if delta_usec < NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC:
        return NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC
    if delta_usec > NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC:
        return NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC
    return int(delta_usec)


# ---------------------------------------------------------------------------
# Histogram helpers
# ---------------------------------------------------------------------------


def _empty_histogram_for_field(field: bytes, query: ExplorerQuery) -> ExplorerHistogram:
    """Build an empty histogram for `field` using the chunk-1 explorer
    helper `_new_histogram`, which mirrors the Rust `new_histogram`.
    """

    return _new_histogram(field, query)


def _merge_histogram(
    target: "Optional[ExplorerHistogram]",
    source: ExplorerHistogram,
) -> ExplorerHistogram:
    """Mirror Rust `merge_histogram` (L2621-2655).

    Sum per-bucket values across files; the bucket *positions* (start/end
    realtime) are taken from the first non-None histogram and must match
    exactly across subsequent merges.
    """

    if target is None:
        return ExplorerHistogram(
            field=bytes(source.field),
            buckets=[
                ExplorerHistogramBucket(
                    start_realtime_usec=int(b.start_realtime_usec),
                    end_realtime_usec=int(b.end_realtime_usec),
                    values={bytes(k): int(v) for k, v in b.values.items()},
                )
                for b in source.buckets
            ],
        )
    if target.field != source.field or len(target.buckets) != len(source.buckets):
        return target
    for idx, src_bucket in enumerate(source.buckets):
        dst_bucket = target.buckets[idx]
        if (
            dst_bucket.start_realtime_usec != src_bucket.start_realtime_usec
            or dst_bucket.end_realtime_usec != src_bucket.end_realtime_usec
        ):
            return target
        for value, count in src_bucket.values.items():
            dst_bucket.values[value] = dst_bucket.values.get(value, 0) + int(count)
    return target


# ---------------------------------------------------------------------------
# Combined-result accumulator
# ---------------------------------------------------------------------------


@dataclass
class LocatedRow:
    """A row tagged with the file it came from (Rust L1653-1657)."""

    file_path: str
    row_realtime_usec: int
    row_cursor: str
    row_payloads: List[bytes] = field(default_factory=list)


@dataclass
class CombinedResult:
    """Mirror of `CombinedResult` (Rust L1828-1844).

    Holds the merged per-file explorer results: rows (with file tags),
    facet counts, histogram, column fields, and a merged stats
    accumulator. The merge function below implements the exact
    aggregate-vs-last semantics documented at the top of this section.
    """

    rows: List[LocatedRow] = field(default_factory=list)
    facets: Dict[bytes, Dict[bytes, int]] = field(default_factory=dict)
    histogram: "Optional[ExplorerHistogram]" = None
    column_fields: set = field(default_factory=set)
    stats: ExplorerStats = field(default_factory=ExplorerStats)
    matched_files: int = 0
    matched_paths: List[str] = field(default_factory=list)
    skipped_files: int = 0
    file_errors: List[str] = field(default_factory=list)
    partial: bool = False
    timed_out: bool = False
    cancelled: bool = False
    sampling_enabled: bool = False

    def merge(
        self,
        path: str,
        result: ExplorerResult,
        direction: Direction,
        limit: int,
    ) -> None:
        """Mirror Rust `CombinedResult::merge` (L2124-2165)."""

        if result.histogram is not None:
            self.histogram = _merge_histogram(self.histogram, result.histogram)
        self._merge_stats(result.stats)
        for row in result.rows:
            self.rows.append(
                LocatedRow(
                    file_path=path,
                    row_realtime_usec=int(row.realtime_usec),
                    row_cursor=str(row.cursor),
                    row_payloads=list(row.payloads),
                )
            )
        for field in result.column_fields:
            if isinstance(field, str):
                self.column_fields.add(field.encode("utf-8"))
            else:
                self.column_fields.add(bytes(field))
        for field, values in result.facets.items():
            dest = self.facets.setdefault(bytes(field), {})
            for value, count in values.items():
                dest[bytes(value)] = dest.get(bytes(value), 0) + int(count)
        self._sort_and_limit(direction, limit)

    def _merge_stats(self, stats: ExplorerStats) -> None:
        """Mirror Rust `merge_stats` (L2220-2297) exactly: sum everything
        except the two maxima noted in the module docstring.
        """

        s = self.stats
        s.rows_examined += stats.rows_examined
        s.rows_matched += stats.rows_matched
        s.facet_rows_matched += stats.facet_rows_matched
        s.rows_returned += stats.rows_returned
        s.rows_unsampled += stats.rows_unsampled
        s.rows_estimated += stats.rows_estimated
        s.sampling_sampled += stats.sampling_sampled
        s.sampling_unsampled += stats.sampling_unsampled
        s.sampling_estimated += stats.sampling_estimated
        if stats.last_realtime_usec > s.last_realtime_usec:
            s.last_realtime_usec = stats.last_realtime_usec
        if stats.max_source_realtime_delta_usec > s.max_source_realtime_delta_usec:
            s.max_source_realtime_delta_usec = stats.max_source_realtime_delta_usec
        s.data_refs_seen += stats.data_refs_seen
        s.data_refs_skipped += stats.data_refs_skipped
        s.data_payloads_loaded += stats.data_payloads_loaded
        s.data_objects_classified += stats.data_objects_classified
        s.data_cache_hits += stats.data_cache_hits
        s.data_cache_misses += stats.data_cache_misses
        s.payloads_decompressed += stats.payloads_decompressed
        s.fts_scans += stats.fts_scans
        s.facet_updates += stats.facet_updates
        s.histogram_updates += stats.histogram_updates
        s.returned_row_expansions += stats.returned_row_expansions
        s.early_stop_opportunities += stats.early_stop_opportunities
        s.early_stops += stats.early_stops

    def _sort_and_limit(self, direction: Direction, limit: int) -> None:
        """Mirror Rust `sort_and_limit` (L2174-2186) + the
        `make_row_timestamps_unique` pass that follows.
        """

        if direction == Direction.FORWARD:
            self.rows.sort(key=lambda r: r.row_realtime_usec)
        else:
            self.rows.sort(key=lambda r: r.row_realtime_usec, reverse=True)
        # Make row timestamps unique (mirror make_row_timestamps_unique).
        last_from = 0
        last_to = 0
        initialized = False
        for located in self.rows:
            ts = located.row_realtime_usec
            if initialized and last_from <= ts <= last_to:
                if direction == Direction.BACKWARD:
                    last_from = max(0, last_from - 1)
                    located.row_realtime_usec = last_from
                else:
                    last_to += 1
                    located.row_realtime_usec = last_to
            else:
                last_from = ts
                last_to = ts
                initialized = True
        if limit and len(self.rows) > limit:
            self.rows = self.rows[:limit]
        self.stats.rows_returned = len(self.rows)

    def add_zero_count_facet_values(
        self,
        vocabulary: "Mapping[bytes, Mapping[bytes, int]]",
    ) -> None:
        """Mirror Rust `add_zero_count_facet_values` (L2299-2309). For
        each (field, value) key in the supplied vocabulary, register
        the value in the merged facets map with a zero count if it is
        not already present. This widens the facet vocabulary to
        include values that exist in the unfiltered scan but were
        never matched in the filtered one.
        """

        for field, values in vocabulary.items():
            field_bytes = bytes(field)
            target = self.facets.setdefault(field_bytes, {})
            for value in values.keys():
                value_bytes = bytes(value)
                if value_bytes in target:
                    continue
                target[value_bytes] = 0

    def add_zero_count_facet_values_from_files(
        self,
        fields: Sequence[bytes],
        reader_options: "Any",
    ) -> None:
        """Mirror Rust `add_zero_count_facet_values_from_files`
        (L2311-2336). For each matched journal file, walk the FIELD
        hash table for every requested facet field and register each
        unique value as a zero-count entry in the merged facets map.
        Mirrors the same `add_netdata_facet_count` semantics: the
        value is added to the running count if already present, and
        inserted with zero if not. Values that fail UTF-8 decoding
        or whose reader cannot be opened are silently skipped
        (matching the Rust `if let Ok ... else { continue; }` paths).
        """

        if not fields or not self.matched_paths:
            return
        from .reader import FileReader

        for path_str in self.matched_paths:
            try:
                reader = FileReader.open(path_str)
            except Exception:
                continue
            try:
                for field in fields:
                    if not field:
                        continue
                    try:
                        field_name = field.decode("utf-8")
                    except UnicodeDecodeError:
                        continue
                    try:
                        values = reader.query_unique(field_name)
                    except Exception:
                        continue
                    if not values:
                        continue
                    target = self.facets.setdefault(bytes(field), {})
                    for value in values:
                        if not value or value == b"-":
                            continue
                        existing = target.get(value)
                        if existing is None:
                            target[value] = 0
            finally:
                try:
                    reader.close()
                except Exception:
                    pass

    def add_zero_count_selected_filter_values(
        self, request: "NetdataRequest",
    ) -> None:
        """Mirror Rust `add_zero_count_selected_filter_values`
        (L2338-2352). For every request filter whose field is in
        the reportable facet set (request.facets or the histogram
        field), register each selected filter value in the merged
        facets map. The value is added to the running count if
        already present, and inserted with zero if not. This is
        what makes a filter like `PRIORITY=3` still surface
        `PRIORITY=3` in the PRIORITY facet even when no rows
        match the filter (or the field has only values that the
        filter excludes).
        """

        if not request.filters:
            return
        report_fields: set = set(bytes(f) for f in request.facets)
        if request.histogram is not None:
            report_fields.add(request.histogram.encode("utf-8"))
        for filter_ in request.filters:
            field_bytes = bytes(filter_.field)
            if field_bytes not in report_fields:
                continue
            target = self.facets.setdefault(field_bytes, {})
            for value in filter_.values:
                value_bytes = bytes(value)
                existing = target.get(value_bytes)
                if existing is None:
                    target[value_bytes] = 0


# ---------------------------------------------------------------------------
# Netdata request decoding
# ---------------------------------------------------------------------------


def _get_bool(request: Mapping[str, Any], key: str, default: bool = False) -> bool:
    value = request.get(key)
    if isinstance(value, bool):
        return value
    return default


def _get_i64(request: Mapping[str, Any], key: str) -> "Optional[int]":
    value = request.get(key)
    if value is None:
        return None
    if isinstance(value, bool):
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _get_u64(request: Mapping[str, Any], key: str) -> "Optional[int]":
    value = request.get(key)
    if value is None:
        return None
    if isinstance(value, bool):
        return None
    try:
        v = int(value)
    except (TypeError, ValueError):
        return None
    if v < 0:
        return None
    return v


def _get_str(request: Mapping[str, Any], key: str) -> "Optional[str]":
    value = request.get(key)
    if isinstance(value, str):
        return value
    return None


def _parse_string_array(value: Any) -> "Optional[List[str]]":
    if not isinstance(value, list):
        return None
    return [item for item in value if isinstance(item, str)]


def _request_direction(request: Mapping[str, Any]) -> Direction:
    value = _get_str(request, "direction") or "backward"
    if value in ("forward", "forwards", "next"):
        return Direction.FORWARD
    return Direction.BACKWARD


def _request_limit(request: Mapping[str, Any]) -> int:
    value = _get_u64(request, "last")
    if value is None or value == 0:
        return DEFAULT_ITEMS_TO_RETURN
    return int(value)


def _request_facets(
    request: Mapping[str, Any], config: NetdataFunctionConfig
) -> List[bytes]:
    facets = _parse_string_array(request.get("facets"))
    if facets is None:
        return [f.encode("utf-8") if isinstance(f, str) else bytes(f) for f in config.default_facets]
    return [f.encode("utf-8") for f in facets]


def _request_histogram(request: Mapping[str, Any]) -> "Optional[str]":
    value = _get_str(request, "histogram")
    if value is None or value == "":
        return None
    return value


def _request_histogram_or_default(
    requested: "Optional[str]", config: NetdataFunctionConfig
) -> "Optional[str]":
    if requested is not None:
        return requested
    return config.default_histogram


def _request_query(request: Mapping[str, Any]) -> "Optional[str]":
    value = _get_str(request, "query")
    if value is None or value == "":
        return None
    return value


def _normalize_filter_value(field: str, value: str) -> bytes:
    """Mirror Rust `normalize_filter_value` (L3467-3474).

    For PRIORITY, decode textual names ("err", "info", ...) back to
    numeric strings. Other fields pass through.
    """

    if field == "PRIORITY":
        number = _priority_name_to_number(value)
        if number is not None:
            return str(number).encode("ascii")
    return value.encode("utf-8")


def _priority_name_to_number(name: str) -> "Optional[int]":
    """Inverse of `_priority_name`."""

    table = {
        "panic": 0, "alert": 1, "critical": 2, "error": 3,
        "warning": 4, "notice": 5, "info": 6, "debug": 7,
    }
    return table.get(name)


def _parse_filters(value: Any) -> List["ExplorerFilter"]:
    """Mirror Rust `parse_filters` (L3360-3380)."""

    from .explorer import ExplorerFilter
    if not isinstance(value, dict):
        return []
    out: List[ExplorerFilter] = []
    for field, raw_values in value.items():
        if field in ("query", "source", "__logs_sources"):
            continue
        values = _parse_string_array(raw_values)
        if not values:
            continue
        if not isinstance(field, str) or not field:
            continue
        normalized = [_normalize_filter_value(field, v) for v in values]
        out.append(ExplorerFilter(field=field.encode("utf-8"), values=normalized))
    return out


def _source_type_for_name(value: str) -> "Optional[int]":
    table = {
        "all": NETDATA_SOURCE_TYPE_ALL,
        "all-local-logs": NETDATA_SOURCE_TYPE_LOCAL_ALL,
        "all-remote-systems": NETDATA_SOURCE_TYPE_REMOTE_ALL,
        "all-local-system-logs": NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
        "all-local-user-logs": NETDATA_SOURCE_TYPE_LOCAL_USER,
        "all-local-namespaces": NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
        "all-uncategorized": NETDATA_SOURCE_TYPE_LOCAL_OTHER,
    }
    return table.get(value)


def _parse_source_selection(value: Any) -> "Tuple[int, List[str]]":
    """Mirror Rust `parse_source_selection` (L3388-3407)."""

    source_type = NETDATA_SOURCE_TYPE_ALL
    exact: List[str] = []
    if not isinstance(value, dict):
        return source_type, exact
    raw = value.get("__logs_sources")
    values = _parse_string_array(raw)
    if not values:
        return source_type, exact
    source_type = 0
    for v in values:
        mapped = _source_type_for_name(v)
        if mapped is None:
            exact.append(v)
        else:
            source_type |= mapped
    return source_type, exact


@dataclass
class NetdataRequest:
    """Decoded Netdata function request.

    All 16 accepted parameters are decoded. `data_only`, `delta`,
    `tail`, and `if_modified_since_usec` are decoded for echo and
    downstream contract; behavior for the data-only short-circuit is
    not in scope of this chunk (the full data path runs regardless).
    """

    info: bool
    echo: Dict[str, Any]
    after_realtime_usec: "Optional[int]"
    before_realtime_usec: "Optional[int]"
    if_modified_since_usec: int
    anchor: ExplorerAnchor
    direction: Direction
    limit: int
    data_only: bool
    delta: bool
    tail: bool
    sampling: int
    source_type: int
    exact_sources: List[str]
    filters: List["ExplorerFilter"]
    facets: List[bytes]
    histogram: "Optional[str]"
    query: "Optional[str]"
    # Track which keys were explicitly set in the original request
    # body, so downstream code can distinguish user-supplied from
    # defaulted values (e.g. for the data_only histogram suppression).
    _explicit_keys: set = field(default_factory=set)

    @staticmethod
    def parse(
        value: Mapping[str, Any], config: NetdataFunctionConfig
    ) -> "NetdataRequest":
        info = _get_bool(value, "info", False)
        after = _get_i64(value, "after")
        before = _get_i64(value, "before")
        parse_now = _unix_now_seconds()
        after_usec, before_usec = _normalize_time_window(
            parse_now, after, before
        )
        direction = _request_direction(value)
        if_modified = _get_u64(value, "if_modified_since") or 0
        data_only = _get_bool(value, "data_only", False)
        delta = data_only and _get_bool(value, "delta", False)
        tail = (
            data_only
            and if_modified != 0
            and _get_bool(value, "tail", False)
        )
        sampling = _get_u64(value, "sampling")
        if sampling is None:
            sampling = DEFAULT_ITEMS_SAMPLING
        anchor_value = _get_u64(value, "anchor")
        if anchor_value is not None and anchor_value != 0:
            anchor = ExplorerAnchor.realtime(anchor_value)
        else:
            anchor = ExplorerAnchor.auto()
        if tail and anchor.kind == ExplorerAnchorKind.REALTIME:
            direction = Direction.BACKWARD
        requested_limit = _request_limit(value)
        # The Rust floor is `limit.max(2)`; the chunks-2a defaults are
        # already 2+. Mirror exactly.
        limit = max(2, requested_limit)
        requested_facets = _parse_string_array(value.get("facets"))
        facets = _request_facets(value, config)
        requested_histogram = _request_histogram(value)
        histogram = _request_histogram_or_default(requested_histogram, config)
        requested_query = _request_query(value)
        source_type, exact_sources = _parse_source_selection(
            value.get("selections")
        )
        filters = _parse_filters(value.get("selections"))
        echo = _build_echo(
            info=info,
            after_usec=after_usec,
            before_usec=before_usec,
            if_modified=if_modified,
            anchor=anchor,
            direction=direction,
            limit=requested_limit,
            data_only=data_only,
            delta=delta,
            tail=tail,
            sampling=int(sampling),
            source_type=source_type,
            requested_facets=requested_facets,
            selections=value.get("selections"),
            histogram=requested_histogram,
            query=requested_query,
        )
        explicit = set(value.keys()) if isinstance(value, Mapping) else set()
        return NetdataRequest(
            info=info,
            echo=echo,
            after_realtime_usec=after_usec,
            before_realtime_usec=before_usec,
            if_modified_since_usec=if_modified,
            anchor=anchor,
            direction=direction,
            limit=limit,
            data_only=data_only,
            delta=delta,
            tail=tail,
            sampling=int(sampling),
            source_type=source_type,
            exact_sources=exact_sources,
            filters=filters,
            facets=facets,
            histogram=histogram,
            query=requested_query,
            _explicit_keys=explicit,
        )

    def to_explorer_query(
        self,
        matched_files: int,
        after_override: "Optional[int]" = None,
        before_override: "Optional[int]" = None,
        file_header: "Optional[Mapping[str, Any]]" = None,
        realtime_slack_usec: "Optional[int]" = None,
    ) -> ExplorerQuery:
        """Build the chunk-1 ExplorerQuery for a file scan.

        Mirrors Rust `NetdataRequest::to_explorer_query` (L1519-1614)
        for the tail-anchor / backward-page-anchor / sampling math
        (SOW-0093 + SOW-0104 chunk 2c). `after_override` and
        `before_override` are ignored unless provided; when provided
        they bypass the tail-after / backward-page-anchor clamping.
        The full clamp is applied when the overrides are None and
        the request has a realtime anchor.
        """

        analysis_enabled = not self.data_only or self.delta
        tail_anchor = (
            self.tail
            and self.anchor.kind == ExplorerAnchorKind.REALTIME
        )
        backward_page_anchor = (
            self.data_only
            and not tail_anchor
            and self.direction == Direction.BACKWARD
            and self.anchor.kind == ExplorerAnchorKind.REALTIME
        )
        if after_override is not None:
            after = after_override
        elif tail_anchor:
            after = _tail_after_realtime_bound(
                self.after_realtime_usec, self.anchor
            )
        else:
            after = self.after_realtime_usec
        if before_override is not None:
            before = before_override
        elif backward_page_anchor:
            before = _before_realtime_bound_excluding_anchor(
                self.before_realtime_usec, self.anchor
            )
        else:
            before = self.before_realtime_usec
        anchor_for_query = (
            ExplorerAnchor.auto()
            if (tail_anchor or backward_page_anchor)
            else self.anchor
        )
        sampling: "Optional[ExplorerSampling]" = None
        if (
            analysis_enabled
            and self.sampling != 0
            and matched_files != 0
            and after is not None
            and before is not None
        ):
            sampling = ExplorerSampling(
                budget=int(self.sampling),
                matched_files=int(matched_files),
            )
            if file_header is not None:
                _fill_sampling_from_header(sampling, file_header)
        query = ExplorerQuery()
        query.after_realtime_usec = after
        query.before_realtime_usec = before
        query.anchor = anchor_for_query
        query.direction = self.direction
        query.limit = self.limit
        query.filters = list(self.filters)
        if analysis_enabled:
            query.facets = [bytes(f) for f in self.facets]
            if self.histogram is not None:
                query.histogram = self.histogram.encode("utf-8")
        else:
            query.facets = []
            query.histogram = None
        query.histogram_after_realtime_usec = self.after_realtime_usec
        query.histogram_before_realtime_usec = self.before_realtime_usec
        query.histogram_target_buckets = DEFAULT_HISTOGRAM_BUCKETS
        query.field_mode = ExplorerFieldMode.FIRST_VALUE
        query.exclude_facet_field_filters = len(
            {f.field for f in self.filters}
        ) > 1
        query.use_source_realtime = True
        if realtime_slack_usec is None:
            realtime_slack_usec = NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC
        query.realtime_slack_usec = _normalize_journal_vs_realtime_delta_usec(
            realtime_slack_usec
        )
        # stop_when_rows_full: data_only && !tail_anchor (matches Rust L1609).
        # Delta+data_only disables the per-file stop to allow full scan.
        query.stop_when_rows_full = self.data_only and not tail_anchor
        query.stop_when_rows_full_check_every = DATA_ONLY_CHECK_EVERY_ROWS
        query.sampling = sampling
        return query


def _build_echo(
    *,
    info: bool,
    after_usec: "Optional[int]",
    before_usec: "Optional[int]",
    if_modified: int,
    anchor: ExplorerAnchor,
    direction: Direction,
    limit: int,
    data_only: bool,
    delta: bool,
    tail: bool,
    sampling: int,
    source_type: int,
    requested_facets: "Optional[List[str]]",
    selections: Any,
    histogram: "Optional[str]",
    query: "Optional[str]",
) -> Dict[str, Any]:
    """Mirror Rust `normalized_request_echo` (L3711-3762).

    The echo is the canonical request snapshot returned in `_request`
    so downstream fixtures can compare requests without re-deriving
    time-window math.
    """

    anchor_usec = (
        anchor.realtime_usec if anchor.kind == ExplorerAnchorKind.REALTIME else 0
    )
    direction_name = "forward" if direction == Direction.FORWARD else "backward"
    after_seconds = (int(after_usec) // 1_000_000) if after_usec is not None else 0
    before_seconds = (int(before_usec) // 1_000_000) if before_usec is not None else 0
    out: Dict[str, Any] = {
        "info": info,
        "slice": True,
        "data_only": data_only,
        "delta": delta,
        "tail": tail,
        "sampling": int(sampling),
        "source_type": int(source_type),
        "after": after_seconds,
        "before": before_seconds,
        "if_modified_since": int(if_modified),
        "anchor": int(anchor_usec),
        "direction": direction_name,
        "last": int(limit),
        "query": query,
        "histogram": histogram,
    }
    if requested_facets is not None:
        out["facets"] = list(requested_facets)
    if isinstance(selections, dict):
        selections_copy = dict(selections)
        sources = selections_copy.get("__logs_sources")
        if isinstance(sources, list):
            selections_copy["__logs_sources"] = [None for _ in sources]
        out["selections"] = selections_copy
    return out


# ---------------------------------------------------------------------------
# Source-selector response + journal source summary
# ---------------------------------------------------------------------------


NETDATA_EMPTY_STRING_FACET_HASH_ID = "CzGfAU2z3TC"
NETDATA_UNAVAILABLE_FIELD_LABEL = "[unavailable field]"


def _min_realtime(current: "Optional[int]", candidate: int) -> int:
    """Return ``min(current, candidate)`` treating ``None`` as +infinity.

    Mirrors the `map_or` + `min` widen pattern used by
    `JournalSourceSummary::add_path` (Rust L1741-1752). Inline so the
    summary path avoids repeated option-boxing overhead.
    """

    if current is None:
        return int(candidate)
    return int(current) if int(current) < int(candidate) else int(candidate)


def _max_realtime(current: "Optional[int]", candidate: int) -> int:
    """Return ``max(current, candidate)`` treating ``None`` as -infinity.

    Mirrors the `map_or` + `max` widen pattern used by
    `JournalSourceSummary::add_path` (Rust L1747-1752). Inline so the
    summary path avoids repeated option-boxing overhead.
    """

    if current is None:
        return int(candidate)
    return int(current) if int(current) > int(candidate) else int(candidate)


def _read_file_header_realtime_bounds(path: str) -> "tuple[int, int]":
    """Return ``(head_entry_realtime, tail_entry_realtime)`` from the file.

    Mirrors the `FileReader::open_with_options` + `reader.header()`
    step used by Rust `JournalSourceSummary::add_path` (L1757-1776):
    the reader captures the header snapshot without scanning entries.
    In Python the cheapest equivalent is the `FileReader.open` prologue
    (decompress `.journal.zst` to a temp file, mmap the file,
    `parse_file_header`) — both bounds default to 0 for files with no
    entries, which the caller skips via the `!= 0` guard. Any failure
    (missing file, invalid header, too small) returns ``(0, 0)`` so the
    summary path leaves the bounds untouched and the file still
    contributes `files` + `total_size` from the prior `os.stat`.
    """

    from .compress import is_zst_file, decompress_zst_to_temp
    from .header import parse_file_header, HEADER_MIN_SIZE

    cleanup_dir: "Optional[str]" = None
    mapped = None
    fd: "Optional[int]" = None
    try:
        open_path = str(path)
        if is_zst_file(open_path):
            cleanup_dir = os.path.dirname(decompress_zst_to_temp(open_path))
            open_path = os.path.join(cleanup_dir, "decompressed.journal")
        fd = os.open(open_path, os.O_RDONLY)
        try:
            size = os.fstat(fd).st_size
            if size < HEADER_MIN_SIZE:
                return (0, 0)
            mapped = mmap.mmap(fd, 0, access=mmap.ACCESS_READ)
        finally:
            if mapped is None:
                os.close(fd)
                fd = None
        header = parse_file_header(mapped)
        return (
            int(header.get("head_entry_realtime", 0) or 0),
            int(header.get("tail_entry_realtime", 0) or 0),
        )
    except Exception:
        return (0, 0)
    finally:
        if mapped is not None:
            try:
                mapped.close()
            except Exception:
                pass
        if fd is not None:
            try:
                os.close(fd)
            except Exception:
                pass
        if cleanup_dir is not None:
            try:
                import shutil
                shutil.rmtree(cleanup_dir, ignore_errors=True)
            except Exception:
                pass


@dataclass
class JournalSourceSummary:
    """Mirror of `JournalSourceSummary` (Rust L1705-1799).

    Accumulates `files`, `total_size`, and the union time range
    (`first_realtime_usec`, `last_realtime_usec`) across a set of
    files in a single source bucket. `add_path` is the only
    mutator: a file is counted once and its size added, then the
    time range is widened using `min(first)` and `max(last)`.
    """

    files: int = 0
    total_size: int = 0
    first_realtime_usec: "Optional[int]" = None
    last_realtime_usec: "Optional[int]" = None

    def add_path(
        self,
        path: str,
        metadata: "Optional[NetdataJournalFileMetadata]" = None,
    ) -> None:
        """Mirror Rust `JournalSourceSummary::add_path` (L1728-1777).

        If the file cannot be stat-ed the call is a no-op (mirrors
        Rust: `if let Ok(metadata) = std::fs::metadata(path)`). When
        the stat succeeds the file contributes `files` + `total_size`,
        and `first_realtime_usec` / `last_realtime_usec` are widened
        from the optional `NetdataJournalFileMetadata` cache and, when
        either bound is still missing, from a header-only read of the
        file (Rust opens the file via `FileReader::open_with_options`
        which captures the header snapshot). The header path is the
        cheapest way to obtain the first/last entry realtime without
        scanning entries: it mmaps the file, parses the journal
        header, and closes; entries are never decoded.
        `head_entry_realtime == 0` (zero-entry file) and
        `tail_entry_realtime == 0` are skipped, mirroring the Rust
        guards. Files whose header cannot be parsed (too small, bad
        magic) still contribute `files` + `total_size` from the stat
        but contribute no bounds; the resulting summary renders
        `covering off, last entry at unknown`.
        """

        try:
            stat = os.stat(path)
        except OSError:
            return
        self.files += 1
        self.total_size += int(stat.st_size)
        if metadata is not None:
            metadata_first = metadata.msg_first_realtime_usec
            metadata_last = metadata.msg_last_realtime_usec
            if metadata_first is not None:
                self.first_realtime_usec = _min_realtime(
                    self.first_realtime_usec, metadata_first
                )
            if metadata_last is not None:
                self.last_realtime_usec = _max_realtime(
                    self.last_realtime_usec, metadata_last
                )
            if metadata_first is not None and metadata_last is not None:
                return
        head_realtime, tail_realtime = _read_file_header_realtime_bounds(path)
        if head_realtime != 0:
            self.first_realtime_usec = _min_realtime(
                self.first_realtime_usec, head_realtime
            )
        if tail_realtime != 0:
            self.last_realtime_usec = _max_realtime(
                self.last_realtime_usec, tail_realtime
            )


def _human_binary_size(num_bytes: int) -> str:
    """Mirror Rust `human_binary_size` (L3785-3807)."""

    units = ("B", "KiB", "MiB", "GiB", "TiB")
    value = float(num_bytes)
    unit = 0
    while value >= 1024.0 and unit + 1 < len(units):
        value /= 1024.0
        unit += 1
    if unit == 0:
        return f"{num_bytes}{units[unit]}"
    if value == int(value):
        return f"{int(value)}{units[unit]}"
    text = f"{value:.2f}"
    while "." in text and text.endswith("0"):
        text = text[:-1]
    if text.endswith("."):
        text = text[:-1]
    return f"{text}{units[unit]}"


def _human_duration_seconds(seconds: int) -> str:
    """Mirror Rust `human_duration_seconds` (L3809-3839).

    Units are 1y=365d, 1mo=30d, 1d=86400s, 1h=3600s, 1m=60s, 1s=1s.
    Components whose value is 0 are omitted. If every component is
    zero, the seconds component is emitted as ``0s``. Components are
    joined with a single ASCII space.
    """

    remaining = int(seconds)
    years = remaining // (365 * 86_400)
    remaining = remaining % (365 * 86_400)
    months = remaining // (30 * 86_400)
    remaining = remaining % (30 * 86_400)
    days = remaining // 86_400
    remaining = remaining % 86_400
    hours = remaining // 3600
    remaining = remaining % 3600
    minutes = remaining // 60
    secs = remaining % 60
    parts: List[str] = []
    if years:
        parts.append(f"{years}y")
    if months:
        parts.append(f"{months}mo")
    if days:
        parts.append(f"{days}d")
    if hours:
        parts.append(f"{hours}h")
    if minutes:
        parts.append(f"{minutes}m")
    if secs or not parts:
        parts.append(f"{secs}s")
    return " ".join(parts)


def _format_last_entry_rfc3339_usec(last_realtime_usec: "Optional[int]") -> str:
    """Mirror the `last entry at <...>` formatter from Rust
    ``JournalSourceSummary::info`` (L1786-1790).

    ``last_realtime_usec`` is converted to whole seconds (integer
    division by 1_000_000) and rendered as ``YYYY-MM-DDTHH:MM:SSZ``.
    Negative values, overflow, and ``None`` all map to the literal
    ``unknown`` string used by the Rust side.
    """

    if last_realtime_usec is None:
        return "unknown"
    seconds = int(last_realtime_usec) // 1_000_000
    try:
        dt = datetime.fromtimestamp(seconds, tz=timezone.utc)
    except (OverflowError, OSError, ValueError):
        return "unknown"
    return dt.strftime("%Y-%m-%dT%H:%M:%SZ")


def _summary_to_source_option(
    name: str, summary: "JournalSourceSummary"
) -> "Optional[Dict[str, Any]]":
    """Render a ``JournalSourceSummary`` into a Netdata source option.

    Mirrors the closure inside Rust `push_source_option` and
    `JournalSourceSummary::info` (L1779-1811). Returns ``None`` when
    the summary has no files, so callers can drop empty aggregates
    from the option list.
    """

    if summary.files == 0:
        return None
    first_usec = summary.first_realtime_usec
    last_usec = summary.last_realtime_usec
    if (
        first_usec is not None
        and last_usec is not None
        and last_usec > first_usec
        and (last_usec - first_usec) >= 1_000_000
    ):
        coverage = _human_duration_seconds((last_usec - first_usec) // 1_000_000)
    else:
        coverage = "off"
    last_entry = _format_last_entry_rfc3339_usec(last_usec)
    return {
        "id": name,
        "name": name,
        "info": (
            f"{summary.files} files, total size {_human_binary_size(summary.total_size)}, "
            f"covering {coverage}, last entry at {last_entry}"
        ),
        "pill": _human_binary_size(summary.total_size),
    }


def _build_source_summary(
    paths: _Iterable[str],
    state: "Optional[NetdataFunctionState]" = None,
) -> Dict[str, Any]:
    """Build the `required_source_params` summary (Rust L1148-1223).

    Returns the JSON-friendly list of source options, with one entry
    per logical source plus the canonical seven aggregates
    (all, all-local-logs, all-local-namespaces, all-local-system-logs,
    all-local-user-logs, all-remote-systems, all-uncategorized). Files
    that share a namespace parent or a remote source name are merged
    into a single per-source bucket.

    `state` is the optional `NetdataFunctionState` hook consulted once
    per file to seed the per-file `NetdataJournalFileMetadata` cache
    exactly like Rust `file_metadata` (L2862-2870). When the state
    provides both `msg_first_realtime_usec` and `msg_last_realtime_usec`
    for a file, the file header is not opened for the summary.
    """

    from collections import OrderedDict

    all_summary = JournalSourceSummary()
    local = JournalSourceSummary()
    local_namespaces = JournalSourceSummary()
    local_system = JournalSourceSummary()
    local_user = JournalSourceSummary()
    remote = JournalSourceSummary()
    other = JournalSourceSummary()
    exact: "OrderedDict[str, JournalSourceSummary]" = OrderedDict()
    for path_str in paths:
        path = pathlib.Path(path_str)
        metadata = _state_file_metadata(state, path_str)
        source_type = _journal_file_source_type(path)
        all_summary.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_LOCAL_ALL:
            local.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE:
            local_namespaces.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_LOCAL_SYSTEM:
            local_system.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_LOCAL_USER:
            local_user.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_REMOTE_ALL:
            remote.add_path(path_str, metadata=metadata)
        if source_type & NETDATA_SOURCE_TYPE_LOCAL_OTHER:
            other.add_path(path_str, metadata=metadata)
        exact_name = _journal_file_exact_source_name(path)
        if exact_name is not None:
            bucket = exact.setdefault(exact_name, JournalSourceSummary())
            bucket.add_path(path_str, metadata=metadata)

    def _summary_to_option(name: str, summary: JournalSourceSummary) -> "Optional[Dict[str, Any]]":
        return _summary_to_source_option(name, summary)

    options: List[Dict[str, Any]] = []
    for label, summary in (
        ("all", all_summary),
        ("all-local-logs", local),
        ("all-local-namespaces", local_namespaces),
        ("all-local-system-logs", local_system),
        ("all-local-user-logs", local_user),
        ("all-remote-systems", remote),
        ("all-uncategorized", other),
    ):
        option = _summary_to_option(label, summary)
        if option is not None:
            options.append(option)
    for name, summary in exact.items():
        option = _summary_to_option(name, summary)
        if option is not None:
            options.append(option)
    return {
        "id": "__logs_sources",
        "options": options,
    }


# ---------------------------------------------------------------------------
# Response builders: info, __logs_sources, full data envelope
# ---------------------------------------------------------------------------


def _build_info_response(
    echo: Mapping[str, Any],
    paths: Sequence[str],
    config: NetdataFunctionConfig,
    state: "Optional[NetdataFunctionState]" = None,
) -> Dict[str, Any]:
    """Mirror Rust `info_response` (L612-636) + `required_source_params`
    (L1148-1223). The info response is shape-compatible with the Rust
    SDK and carries the same `accepted_params`, `required_params`,
    `show_ids`, `has_history`, `pagination`, `status`, `type`, `help`,
    `versions`, and `v` keys. `state` is the optional
    `NetdataFunctionState` hook consulted while building the source
    summary, exactly like Rust `NetdataJournalFunction::run_*` passes
    `&options` into `required_source_params`.
    """

    source_summary = _build_source_summary(paths, state=state)
    return {
        "_request": dict(echo),
        "versions": {"netdata_function_api": 1, "sdk": "0.1.0"},
        "v": 3,
        "accepted_params": list(NETDATA_ACCEPTED_PARAMS),
        "required_params": [
            {
                "id": "__logs_sources",
                "name": config.source_selector_name,
                "help": config.source_selector_help,
                "type": "multiselect",
                "options": source_summary["options"],
            }
        ],
        "show_ids": True,
        "has_history": True,
        "pagination": {
            "enabled": True,
            "key": "anchor",
            "column": "timestamp",
            "units": "timestamp_usec",
        },
        "status": 200,
        "type": "table",
        "help": "Netdata-compatible journal log function backed by the systemd journal SDK",
    }


def _build_logs_sources_response(
    echo: Mapping[str, Any],
    paths: Sequence[str],
    config: NetdataFunctionConfig,
    state: "Optional[NetdataFunctionState]" = None,
) -> Dict[str, Any]:
    """Mirror the `__logs_sources` branch of the Rust `info_response`
    (the `required_source_params` shape). The wire id is the stable
    string `__logs_sources`. `state` is the optional
    `NetdataFunctionState` hook consulted while building the source
    summary.
    """

    return {
        "_request": dict(echo),
        "status": 200,
        "type": "multiselect",
        "id": "__logs_sources",
        "name": config.source_selector_name,
        "help": config.source_selector_help,
        "options": _build_source_summary(paths, state=state)["options"],
    }


def _build_columns_metadata(order: Sequence[str]) -> Dict[str, Any]:
    """Mirror Rust `column_metadata` (L3193-3246)."""

    def _meta(key: str, index: int) -> Dict[str, Any]:
        visible, filter_, full_width = False, "none", False
        column_type = "string"
        visualization = "value"
        if key == "timestamp":
            visible, filter_, column_type = True, "range", "timestamp"
        elif key == "rowOptions":
            visible, filter_, column_type = False, "none", "none"
            visualization = "rowOptions"
        elif key == "_HOSTNAME":
            visible, filter_ = True, "facet"
        elif key in ("ND_JOURNAL_PROCESS", "MESSAGE"):
            visible = True
            if key == "MESSAGE":
                full_width = True
        elif key in ("ND_JOURNAL_FILE", "_SOURCE_REALTIME_TIMESTAMP"):
            pass
        else:
            is_facet = (
                key == "MESSAGE_ID"
                or (
                    "MESSAGE" not in key
                    and "TIMESTAMP" not in key
                    and not key.startswith("__")
                )
            )
            if is_facet:
                filter_ = "facet"
        transform = "datetime_usec" if key == "timestamp" else "none"
        default_value = None if key in ("timestamp", "rowOptions") else "-"
        meta: Dict[str, Any] = {
            "index": int(index),
            "unique_key": key == "timestamp",
            "name": "Timestamp" if key == "timestamp" else key,
            "visible": visible,
            "type": column_type,
            "visualization": visualization,
            "value_options": {
                "transform": transform,
                "decimal_points": 0,
                "default_value": default_value,
            },
            "sort": "ascending",
            "sortable": False,
            "sticky": False,
            "summary": "count",
            "filter": filter_,
            "full_width": full_width,
            "wrap": key != "rowOptions",
            "default_expanded_filter": key in ("PRIORITY", "SYSLOG_FACILITY", "MESSAGE_ID"),
        }
        if key == "rowOptions":
            meta["dummy"] = True
        return meta

    return {key: _meta(key, idx) for idx, key in enumerate(order)}


def _build_query_response(
    request: NetdataRequest,
    config: NetdataFunctionConfig,
    combined: CombinedResult,
    paths: Sequence[str],
    profile: "NetdataFunctionProfile",
) -> Dict[str, Any]:
    """Build the full data-response envelope.

    Mirrors the visible shape of Rust ``base_query_response``
    (L2500-2535), ``add_query_response_metadata`` (L702-720), and
    the ``add_*_if_needed`` family.  Includes the histogram chart
    envelope (summary / totals / result / db / view / agents) with
    ``view.dimensions.names`` always present.
    """

    columns_order = _build_column_order(request, config, combined)
    columns_meta = _build_columns_metadata(columns_order)

    context = DisplayContext()
    data_rows: List[List[Any]] = []
    rows_iter = list(combined.rows)
    if request.direction == Direction.FORWARD:
        rows_iter = list(reversed(rows_iter))
    fields_by_path: Dict[str, Dict[bytes, List[bytes]]] = {}
    for located in combined.rows:
        if located.file_path in fields_by_path:
            continue
        fields_by_path[located.file_path] = _row_fields_map(located)
    for located in rows_iter:
        fields = fields_by_path.get(located.file_path)
        if fields is None:
            fields = _row_fields_map(located)
            fields_by_path[located.file_path] = fields
        data_rows.append(_build_data_row(
            located, columns_order, request.direction, config,
            profile, context,
        ))

    histogram_field = request.histogram
    # For data_only, the default histogram is suppressed unless the
    # request explicitly sets one. Rust L711-715 only emits
    # `available_histograms` from the reportable facet fields when
    # data_only is set, and the `histogram` artifact is `None` for
    # data_only without delta (L736-738).
    if request.data_only and "histogram" not in request._explicit_keys:
        histogram_field = None
    histogram_payload: Any = None
    if histogram_field is not None:
        if combined.histogram is not None:
            histogram_payload = _build_histogram_payload(
                histogram_field, combined.histogram, combined, request
            )
        else:
            empty_query = request.to_explorer_query(combined.matched_files)
            empty = _empty_histogram_for_field(
                histogram_field.encode("utf-8"), empty_query
            )
            histogram_payload = _build_histogram_payload(
                histogram_field, empty, combined, request
            )

    items = _build_items_payload(request, combined, len(combined.rows))
    facets_payload = _build_facets_payload(request, config, combined, profile)
    message = _build_message_payload(combined)
    # Mirror Rust `add_full_query_response_metadata` (L752-772) +
    # `accepted_params_from_fields` (L1139-1146) +
    # `reportable_facet_fields_bytes` (L2358-2366): the query
    # response `accepted_params` is `NETDATA_ACCEPTED_PARAMS`
    # chained with the request's `facets` field names, deduplicated
    # in order. The info response (handled by `_build_info_response`)
    # passes no extra fields and stays at 16.
    accepted = list(NETDATA_ACCEPTED_PARAMS)
    seen: set = set()
    for field_bytes in request.facets:
        try:
            name = field_bytes.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if name in seen:
            continue
        seen.add(name)
        if name not in accepted:
            accepted.append(name)

    body: Dict[str, Any] = {
        "_request": dict(request.echo),
        "versions": {"netdata_function_api": 1, "sdk": "0.1.0"},
        "_journal_files": {
            "matched": int(combined.matched_files),
            "skipped": int(combined.skipped_files),
            "errors": list(combined.file_errors),
        },
        "status": 200,
        "partial": bool(combined.partial),
        "type": "table",
        "show_ids": True,
        "has_history": True,
        "pagination": {
            "enabled": True,
            "key": "anchor",
            "column": "timestamp",
            "units": "timestamp_usec",
        },
        "columns": columns_meta,
        "data": data_rows,
        "_stats": {
            "sdk_explorer": _stats_to_json(combined.stats),
        },
        "expires": _unix_now_seconds() + 3600 if request.data_only else 0,
    }
    # Mirror Rust `add_query_response_metadata` (L702-720):
    # `add_full_query_response_metadata` is called only when
    # `!request.data_only` (L709). For data_only, only the
    # `available_histograms` key (if histogram.is_some()) is added
    # from the reportable facet fields. The other full-mode
    # metadata keys (`message`, `update_every`, `help`,
    # `accepted_params`, `default_sort_column`, `default_charts`)
    # are OMITTED in data_only mode.
    if not request.data_only:
        body["message"] = message
        body["update_every"] = 1
        body["help"] = None
        body["accepted_params"] = accepted
        body["default_sort_column"] = "timestamp"
        body["default_charts"] = []
        body["available_histograms"] = _build_available_histograms(
            request, combined
        )
    elif histogram_field is not None:
        # `available_histograms` is the reportable facet fields,
        # with the explicit histogram field appended last (only
        # in data_only mode). Mirrors Rust L711-716 + L1225-1251.
        body["available_histograms"] = _build_available_histograms(
            request, combined
        )
    # The full analysis outputs are populated from the facets/
    # histogram/items payloads. They are omitted in data_only
    # without delta (mirroring Rust L2602-2611). For data_only
    # + delta, the keys are renamed to the `_delta` variants.
    if not request.data_only or request.delta:
        body["facets"] = facets_payload
        body["histogram"] = histogram_payload
        body["items"] = items
    if not request.data_only or request.tail:
        body["last_modified"] = int(combined.stats.last_realtime_usec)
    # Mirror Rust `add_sampling_if_needed` (L2583-2595): only when
    # sampling was actually enabled for this request.
    if combined.sampling_enabled:
        body["_sampling"] = {
            "enabled": True,
            "sampled": int(combined.stats.sampling_sampled),
            "unsampled": int(combined.stats.sampling_unsampled),
            "estimated": int(combined.stats.sampling_estimated),
        }
    # data_only + delta: rename analysis outputs to the `_delta`
    # variants (Rust L2611-2618).
    if request.data_only and request.delta:
        for old, new in (
            ("facets", "facets_delta"),
            ("histogram", "histogram_delta"),
            ("items", "items_delta"),
        ):
            if old in body:
                body[new] = body.pop(old)
    return body


def _netdata_reorder_key(value: str) -> str:
    """Mirror Rust `netdata_reorder_key` (L4016-4020). The histogram
    palette order is determined by trimming leading ASCII
    punctuation and lowercasing the field name.
    """

    trimmed = value.lstrip("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~")
    return trimmed.lower()


def _build_available_histograms(
    request: NetdataRequest,
    combined: CombinedResult,
) -> List[Dict[str, Any]]:
    """Build the `available_histograms` envelope (Rust L1225-1251).

    The list is the reportable facet fields (request.facets in
    order, deduplicated) for non-data_only, and the same plus the
    explicit histogram field for data_only. The `order` integer
    comes from the field's position in the netdata_reorder_key
    sorted list (1-based).
    """

    seen: "set[bytes]" = set()
    fields: List[bytes] = []
    for field in request.facets:
        if field in seen:
            continue
        seen.add(field)
        fields.append(field)
    if request.data_only and request.histogram is not None:
        histogram_bytes = request.histogram.encode("utf-8")
        if histogram_bytes not in seen:
            seen.add(histogram_bytes)
            fields.append(histogram_bytes)
    # Compute the order using the reorder key (Rust L1232-1238).
    sortable: List[Tuple[str, bytes]] = []
    for field in fields:
        try:
            name = field.decode("utf-8")
        except UnicodeDecodeError:
            name = ""
        sortable.append((_netdata_reorder_key(name), field))
    sortable.sort(key=lambda pair: pair[0])
    order_by_field: Dict[bytes, int] = {
        field: idx + 1 for idx, (_key, field) in enumerate(sortable)
    }
    out: List[Dict[str, Any]] = []
    for field in fields:
        try:
            name = field.decode("utf-8")
        except UnicodeDecodeError:
            continue
        out.append({
            "id": name,
            "name": name,
            "order": int(order_by_field.get(field, 0)),
        })
    return out


def _build_column_order(
    request: NetdataRequest,
    config: NetdataFunctionConfig,
    combined: CombinedResult,
) -> List[str]:
    """Mirror Rust `build_columns` (L774-810): the canonical order
    is `timestamp`, `rowOptions`, the default view keys, the reportable
    facet fields, the histogram field, and finally the column_fields
    discovered by the explorer.
    """

    order: List[str] = ["timestamp", "rowOptions"]
    for key in config.default_view_keys:
        if key not in order:
            order.append(key)
    for field in request.facets:
        name = field.decode("utf-8", errors="replace")
        if name not in order:
            order.append(name)
    if request.histogram is not None and request.histogram not in order:
        order.append(request.histogram)
    for field_bytes in sorted(combined.column_fields):
        try:
            name = field_bytes.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if name not in order:
            order.append(name)
    return order


def _dynamic_process_name(fields: Dict[bytes, List[bytes]]) -> str:
    """Mirror Rust ``dynamic_process_name`` (L3137-3153)."""

    base = ""
    for key in (b"CONTAINER_NAME", b"SYSLOG_IDENTIFIER", b"_COMM"):
        vals = fields.get(key)
        if vals:
            base = vals[0].decode("utf-8", errors="replace")
            break
    if not base:
        return "-"
    pid_vals = fields.get(b"_PID")
    if pid_vals and pid_vals[0]:
        pid = pid_vals[0].decode("utf-8", errors="replace")
        return f"{base}[{pid}]"
    if pid_vals is not None:
        return base
    return f"{base}[-]"


def _row_fields_map(located: LocatedRow) -> Dict[bytes, List[bytes]]:
    """Build a field->values map from a LocatedRow's payloads.

    Each payload is a ``FIELD=value`` byte string.  Multiple values
    for the same field accumulate in the values list, matching the
    Rust ``row_fields`` shape used in ``build_data_rows``.

    Two synthetic columns are injected (mirroring Rust L3124-3133):

    * ``ND_JOURNAL_FILE``  – the source file path from the row.
    * ``ND_JOURNAL_PROCESS`` – derived from CONTAINER_NAME /
      SYSLOG_IDENTIFIER / _COMM plus optional _PID, only when the
      field is not already present in the payloads.
    """

    fields: Dict[bytes, List[bytes]] = {}
    for payload in located.row_payloads:
        eq = payload.find(b"=")
        if eq < 0:
            continue
        field = bytes(payload[:eq])
        value = bytes(payload[eq + 1:])
        fields.setdefault(field, []).append(value)

    fields[b"ND_JOURNAL_FILE"] = [located.file_path.encode("utf-8")]

    if b"ND_JOURNAL_PROCESS" not in fields:
        process = _dynamic_process_name(fields)
        if process:
            fields[b"ND_JOURNAL_PROCESS"] = [process.encode("utf-8")]

    return fields


def _build_data_row(
    located: LocatedRow,
    column_order: Sequence[str],
    direction: Direction,
    config: NetdataFunctionConfig,
    profile: "NetdataFunctionProfile",
    context: DisplayContext,
) -> List[Any]:
    """Mirror Rust ``build_data_rows`` (L812-847): build one output row
    in the canonical column order using the caller-supplied *profile*
    and a per-request *context* for cached uid/gid/boot lookups.
    """

    fields = _row_fields_map(located)
    row: List[Any] = []
    for column in column_order:
        if column == "timestamp":
            row.append(int(located.row_realtime_usec))
        elif column == "rowOptions":
            str_fields = {
                k.decode("utf-8", "replace"): v for k, v in fields.items()
            }
            row.append(profile.row_options(str_fields))
        else:
            values = fields.get(column.encode("utf-8"))
            if not values:
                row.append(None)
                continue
            value = values[0]
            try:
                rendered = profile.field_display_value(
                    context, DisplayScope.Data, column, value
                )
            except Exception:
                rendered = value.decode("utf-8", errors="replace")
            row.append(rendered)
    return row


def _row_options(fields: Mapping[bytes, Sequence[bytes]]) -> Dict[str, str]:
    priority = fields.get(b"PRIORITY")
    if priority:
        return {"severity": _priority_to_row_severity(priority[0])}
    return {"severity": "normal"}


def _build_facets_payload(
    request: NetdataRequest,
    config: NetdataFunctionConfig,
    combined: CombinedResult,
    profile: "NetdataFunctionProfile",
) -> List[Dict[str, Any]]:
    """Mirror Rust `build_facets` (L849-895) at the level required for
    shape parity. Each requested facet is a `{id, name, order, options[]}`
    block; each option is `{id, name, count, order}`.

    Option names are routed through `profile.facet_option_name`
    (Rust L873) so PRIORITY=3 becomes "error", not "3".
    """

    context = DisplayContext()
    out: List[Dict[str, Any]] = []
    for order_index, field in enumerate(request.facets):
        field_name = field.decode("utf-8", errors="replace")
        values = combined.facets.get(field, {})
        options: List[Dict[str, Any]] = []
        for value_bytes, count in values.items():
            if not value_bytes or value_bytes == b"-":
                continue
            try:
                id_str = value_bytes.decode("utf-8", errors="replace")
            except Exception:
                continue
            name = profile.facet_option_name(context, field_name, value_bytes)
            options.append({
                "id": id_str,
                "name": name,
                "count": int(count),
            })
        options.sort(key=lambda o: (-int(o["count"]), str(o["id"])))
        for idx, option in enumerate(options, start=1):
            option["order"] = int(idx)
        out.append({
            "id": field_name,
            "name": field_name,
            "order": int(order_index + 1),
            "options": options,
        })
    return out


def _build_items_payload(
    request: NetdataRequest,
    combined: CombinedResult,
    returned: int,
) -> Dict[str, Any]:
    """Mirror Rust `response_items` (L2537-2559).

    For tail with a realtime anchor, `items.after` includes the +1
    for the exclusive anchor entry (Rust `counters()` L2081-2087:
    ``after = skips_after + shifts`` where skips_after counts the
    anchor row excluded by the tail-after bound).
    """

    unsampled = combined.stats.rows_unsampled
    estimated = combined.stats.rows_estimated
    evaluated = (
        combined.stats.rows_examined + unsampled + estimated
    )
    matched = (
        combined.stats.rows_matched + unsampled + estimated
    )
    raw_after = (
        int(combined.stats.rows_matched - returned)
        if combined.stats.rows_matched > returned
        else 0
    )
    tail_anchor = (
        request.tail
        and request.delta
        and request.anchor.kind == ExplorerAnchorKind.REALTIME
    )
    if tail_anchor:
        raw_after += 1
    return {
        "evaluated": int(evaluated),
        "matched": int(matched),
        "unsampled": int(unsampled),
        "estimated": int(estimated),
        "returned": int(returned),
        "max_to_return": int(request.limit),
        "before": 0,
        "after": raw_after,
    }


def _build_message_payload(combined: CombinedResult) -> Any:
    """Mirror Rust `query_message` (L2376-2424)."""

    if (
        not combined.timed_out
        and combined.stats.rows_unsampled == 0
        and combined.stats.rows_estimated == 0
    ):
        return "OK"
    total = max(
        1,
        combined.stats.rows_examined
        + combined.stats.rows_unsampled
        + combined.stats.rows_estimated,
    )
    real_pct = combined.stats.rows_examined * 100.0 / total
    unsampled_pct = combined.stats.rows_unsampled * 100.0 / total
    estimated_pct = combined.stats.rows_estimated * 100.0 / total
    title_parts: List[str] = []
    description_parts: List[str] = []
    status = "notice"
    if combined.timed_out:
        title_parts.append("Query timed-out, incomplete data. ")
        description_parts.append(
            "QUERY TIMEOUT: The query timed out and may not include all the data "
            "of the selected window. "
        )
        status = "warning"
    if combined.stats.rows_unsampled != 0 or combined.stats.rows_estimated != 0:
        title_parts.append(f"{real_pct:.2f}% real data")
        description_parts.append(
            f"ACTUAL DATA: The filters counters reflect {real_pct:.2f}% of the data. "
        )
    if combined.stats.rows_unsampled != 0:
        title_parts.append(f", {unsampled_pct:.2f}% unsampled")
        description_parts.append(
            f"UNSAMPLED DATA: {unsampled_pct:.2f}% of the events exist and have been "
            f"counted, but their values have not been evaluated, so they are not "
            f"included in the filters counters. "
        )
    if combined.stats.rows_estimated != 0:
        title_parts.append(f", {estimated_pct:.2f}% estimated")
        description_parts.append(
            f"ESTIMATED DATA: The query selected a large amount of data, so to "
            f"avoid delaying too much, the presented data are estimated by "
            f"{estimated_pct:.2f}%. "
        )
    return {
        "title": "".join(title_parts),
        "status": status,
        "description": "".join(description_parts),
    }


def _stats_to_json(stats: ExplorerStats) -> Dict[str, Any]:
    """Serialize ExplorerStats with the same field names the Rust
    SDK uses in `_stats.sdk_explorer`.
    """

    from dataclasses import asdict
    return asdict(stats)


def _build_histogram_payload(
    field: str,
    histogram: ExplorerHistogram,
    combined: CombinedResult,
    request: NetdataRequest,
) -> Dict[str, Any]:
    """Build the histogram chart envelope (Rust L897-1010).

    The envelope keys are `id`, `name`, and `chart` (with `summary`,
    `totals`, `result`, `db`, `view`, `agents`). `view.dimensions.names`
    is always present; for an empty window it is `[]`.

    The dimension id set is built by walking the histogram buckets
    first, then merging in the histogram field's `known_values` from
    the combined facet map (Rust L917-924). The known-values merge
    surfaces zero-count dimensions for the histogram field that the
    filter may have masked (e.g. PRIORITY=6 when the filter is
    PRIORITY=3), so the chart UI can render the full histogram
    palette even for filtered queries.
    """

    if not histogram.buckets:
        return _empty_histogram_chart_envelope(field)

    dimension_ids_set: List[bytes] = []
    seen: set = set()
    for bucket in histogram.buckets:
        for value in bucket.values.keys():
            if value in seen:
                continue
            seen.add(value)
            dimension_ids_set.append(value)
    # Merge the histogram field's facet vocabulary (Rust L917-924).
    # Skip the empty string and the `-` literal (matching
    # `facet_group_is_reportable` L2657-2661).
    histogram_field_bytes = field.encode("utf-8")
    known_values = combined.facets.get(histogram_field_bytes, {})
    for value in known_values.keys():
        if not value or value == b"-":
            continue
        if value in seen:
            continue
        seen.add(value)
        dimension_ids_set.append(value)

    metadata = _histogram_chart_metadata(field, histogram, dimension_ids_set)
    actual_dimensions = metadata["actual_dimensions"]
    data: List[List[Any]] = []
    for bucket in histogram.buckets:
        point: List[Any] = [int(bucket.start_realtime_usec // 1000)]
        for dim_bytes in dimension_ids_set:
            count = bucket.values.get(dim_bytes, 0)
            if count:
                point.append([int(count), 0, 0])
            elif dim_bytes in actual_dimensions:
                point.append([0, 0, 0])
            else:
                point.append([None, 0, 0])
        data.append(point)

    return {
        "id": field,
        "name": field,
        "chart": {
            "summary": metadata["summary"],
            "totals": metadata["totals"],
            "result": {
                "labels": ["time"] + list(metadata["names"]),
                "point": {"value": 0, "arp": 1, "pa": 2},
                "data": data,
            },
            "db": {
                "tiers": 1,
                "update_every": _histogram_update_every_seconds(histogram),
                "units": "events",
                "dimensions": {
                    "ids": metadata["ids_decoded"],
                    "names": metadata["names"],
                    "units": metadata["units"],
                    "sts": metadata["stats"],
                },
                "per_tier": [{
                    "tier": 0,
                    "queries": 1,
                    "points": metadata["points"],
                    "update_every": _histogram_update_every_seconds(histogram),
                }],
            },
            "view": {
                "title": f"Events Distribution by {field}",
                "update_every": _histogram_update_every_seconds(histogram),
                "after": _histogram_after_seconds(histogram),
                "before": _histogram_before_seconds(histogram),
                "units": "events",
                "chart_type": "stackedBar",
                "dimensions": {
                    "grouped_by": ["dimension"],
                    "ids": metadata["ids_decoded"],
                    "names": metadata["names"],
                    "colors": metadata["colors"],
                    "units": metadata["units"],
                    "sts": metadata["stats"],
                },
                "min": metadata["min"],
                "max": metadata["max"],
            },
            "agents": [{
                "mg": "default",
                "nm": "facets.histogram",
                "now": int(time.time()),
                "ai": 0,
            }],
        },
    }


def _empty_histogram_chart_envelope(field: str) -> Dict[str, Any]:
    """Empty histogram chart envelope.

    `view.dimensions.names` is always present (empty list) so the
    chart UI can render an empty window without crashing.
    """

    return {
        "id": field,
        "name": field,
        "chart": {
            "summary": {
                "nodes": [],
                "contexts": [],
                "instances": [],
                "dimensions": [],
                "labels": [],
                "alerts": [],
            },
            "totals": {
                "nodes": {"sl": 1, "qr": 1},
            },
            "result": {
                "labels": ["time"],
                "point": {"value": 0, "arp": 1, "pa": 2},
                "data": [],
            },
            "db": {
                "tiers": 1,
                "update_every": 1,
                "units": "events",
                "dimensions": {
                    "ids": [],
                    "names": [],
                    "units": [],
                    "sts": {
                        "min": [], "max": [], "avg": [], "arp": [], "con": [],
                    },
                },
                "per_tier": [{
                    "tier": 0,
                    "queries": 1,
                    "points": 0,
                    "update_every": 1,
                }],
            },
            "view": {
                "title": f"Events Distribution by {field}",
                "update_every": 1,
                "after": 0,
                "before": 0,
                "units": "events",
                "chart_type": "stackedBar",
                "dimensions": {
                    "grouped_by": ["dimension"],
                    "ids": [],
                    "names": [],
                    "colors": [],
                    "units": [],
                    "sts": {
                        "min": [], "max": [], "avg": [], "arp": [], "con": [],
                    },
                },
                "min": 0,
                "max": 0,
            },
            "agents": [{
                "mg": "default",
                "nm": "facets.histogram",
                "now": int(time.time()),
                "ai": 0,
            }],
        },
    }


def _histogram_chart_metadata(
    field: str,
    histogram: ExplorerHistogram,
    dimension_ids: Sequence[bytes],
) -> Dict[str, Any]:
    """Mirror Rust `histogram_chart_metadata` (L1012-1137)."""

    ids: List[str] = []
    names: List[str] = []
    colors: List[Any] = [None] * len(dimension_ids)
    units: List[str] = ["events"] * len(dimension_ids)
    min_values: List[int] = []
    max_values: List[int] = []
    avg_values: List[float] = []
    arp_values: List[int] = []
    con_values: List[float] = []
    actual_dimensions: set = set()
    for bucket in histogram.buckets:
        for value in bucket.values.keys():
            actual_dimensions.add(value)
    points = 0
    overall_min = 0
    overall_max = 0
    display_context = DisplayContext()
    for dimension in dimension_ids:
        id_str = _decode_display(dimension)
        # Histogram dimension names mirror Rust
        # `systemd_field_display_value(context, scope=Histogram, ...)`
        # (L4329-4400): PRIORITY=3 becomes "error", SYSLOG_FACILITY
        # numeric values become names, etc. The ids stay as the raw
        # payload (matching Rust ids = raw value).
        display = _systemd_field_display_value(
            display_context, DisplayScope.Histogram, field, dimension,
            resolve_user_group_names=False,
        )
        if not isinstance(display, str):
            display = id_str
        d_min, d_max, d_sum, actual = _histogram_dimension_stats(
            histogram, actual_dimensions, dimension
        )
        d_avg = d_sum / len(histogram.buckets) if actual and histogram.buckets else 0.0
        if actual:
            if points == 0 or d_min < overall_min:
                overall_min = d_min
            if d_max > overall_max:
                overall_max = d_max
            points += len(histogram.buckets)
        total = sum(
            histogram.buckets[i].values.get(dimension, 0)
            for i in range(len(histogram.buckets))
        )
        contribution = (d_sum * 100.0 / total) if total > 0 else 0.0
        ids.append(id_str)
        names.append(display)
        min_values.append(d_min)
        max_values.append(d_max)
        avg_values.append(d_avg)
        arp_values.append(0)
        con_values.append(contribution)
    summary_stats = {
        "min": overall_min,
        "max": overall_max,
        "avg": (total / points) if points > 0 else 0.0,
        "con": 100.0,
    }
    totals = {
        "nodes": {"sl": 1, "qr": 1},
    }
    if dimension_ids:
        totals["contexts"] = {"sl": 1, "qr": 1}
        totals["instances"] = {"sl": 1, "qr": 1}
        totals["dimensions"] = {"sl": len(dimension_ids), "qr": len(dimension_ids)}
    summary = {
        "nodes": [{
            "mg": "default",
            "nm": "facets.histogram",
            "ni": 0,
            "st": {"ai": 0, "code": 200, "msg": ""},
            "ds": {"sl": len(dimension_ids), "qr": len(dimension_ids)} if dimension_ids else {},
            "is": {"sl": 1, "qr": 1} if dimension_ids else {},
            "sts": summary_stats if points > 0 else {},
        }],
        "contexts": [{
            "id": "facets.histogram",
            "ds": {"sl": len(dimension_ids), "qr": len(dimension_ids)} if dimension_ids else {},
            "is": {"sl": 1, "qr": 1} if dimension_ids else {},
            "sts": summary_stats if points > 0 else {},
        }],
        "instances": [{
            "id": "facets.histogram",
            "ni": 0,
            "ds": {"sl": len(dimension_ids), "qr": len(dimension_ids)} if dimension_ids else {},
            "sts": summary_stats if points > 0 else {},
        }],
        "dimensions": [
            {
                "id": id_str,
                "nm": name_str,
                "ds": {"sl": 1 if dim in actual_dimensions else 0,
                       "qr": 1 if dim in actual_dimensions else 0},
                "sts": {
                    "min": d_min,
                    "max": d_max,
                    "avg": d_avg,
                    "con": con,
                },
                "pri": idx,
            }
            for idx, (id_str, name_str, dim, d_min, d_max, d_avg, con) in enumerate(zip(
                ids, names, dimension_ids, min_values, max_values, avg_values, con_values
            ))
        ],
        "labels": [],
        "alerts": [],
    }
    return {
        "ids_decoded": ids,
        "names": names,
        "colors": colors,
        "units": units,
        "stats": {
            "min": min_values,
            "max": max_values,
            "avg": avg_values,
            "arp": arp_values,
            "con": con_values,
        },
        "summary": summary,
        "totals": totals,
        "points": int(points),
        "min": int(overall_min),
        "max": int(overall_max),
        "actual_dimensions": actual_dimensions,
    }


def _histogram_dimension_stats(
    histogram: ExplorerHistogram,
    actual_dimensions: set,
    dimension: bytes,
) -> "Tuple[int, int, int, bool]":
    """Mirror Rust `histogram_dimension_stats` (L1268-1290)."""

    if dimension not in actual_dimensions:
        return 0, 0, 0, False
    d_min = 0
    d_max = 0
    d_sum = 0
    for idx, bucket in enumerate(histogram.buckets):
        count = bucket.values.get(dimension, 0)
        if idx == 0 or count < d_min:
            d_min = count
        if count > d_max:
            d_max = count
        d_sum += count
    return d_min, d_max, d_sum, True


def _histogram_update_every_seconds(histogram: ExplorerHistogram) -> int:
    if not histogram.buckets:
        return 1
    first = histogram.buckets[0]
    width = first.end_realtime_usec - first.start_realtime_usec
    if width <= 0:
        width = 1
    return max(1, int(width // 1_000_000))


def _histogram_after_seconds(histogram: ExplorerHistogram) -> int:
    if not histogram.buckets:
        return 0
    return int(histogram.buckets[0].start_realtime_usec // 1_000_000)


def _histogram_before_seconds(histogram: ExplorerHistogram) -> int:
    if not histogram.buckets:
        return 0
    return int(histogram.buckets[-1].end_realtime_usec // 1_000_000)


def _decode_display(value: bytes) -> str:
    return value.decode("utf-8", errors="replace") if isinstance(value, (bytes, bytearray)) else str(value)


# ---------------------------------------------------------------------------
# Function wrapper
# ---------------------------------------------------------------------------


@dataclass
class NetdataFunctionProgress:
    """Mirror `NetdataFunctionProgress` (Rust L274-282)."""

    current_file: int
    total_files: int
    matched_files: int
    skipped_files: int
    stats: ExplorerStats
    elapsed: float


@dataclass
class NetdataFunctionRunOptions:
    """Mirror `NetdataFunctionRunOptions` (Rust L302-336)."""

    timeout: "Optional[float]" = None
    progress_callback: "Optional[Any]" = None
    cancellation_callback: "Optional[Any]" = None
    state: "Optional[NetdataFunctionState]" = None
    progress_interval: float = 0.25

    @staticmethod
    def from_timeout_seconds(seconds: int) -> "NetdataFunctionRunOptions":
        if seconds == 0:
            return NetdataFunctionRunOptions(timeout=None)
        return NetdataFunctionRunOptions(timeout=float(seconds))


class NetdataJournalFunction:
    """Public Netdata-layer function wrapper.

    Mirrors `NetdataJournalFunction<P>` (Rust L268-355): two
    profile-bound constructors and a `new(config, profile)` factory
    that backfills empty selector strings with the defaults. The
    four `run_directory_request_*` methods are the chunk-2b entry
    points: they take a directory plus a request payload, parse it,
    discover journal files, scan each file with the chunk-1 Explorer,
    merge the per-file results via the Rust `CombinedResult`
    semantics, and return the full data-response envelope.
    """

    __slots__ = ("_config", "_profile")

    def __init__(self, config: NetdataFunctionConfig, profile) -> None:
        if not config.source_selector_name:
            config.source_selector_name = DEFAULT_SOURCE_SELECTOR_NAME
        if not config.source_selector_help:
            config.source_selector_help = DEFAULT_SOURCE_SELECTOR_HELP
        self._config = config
        self._profile = profile

    @classmethod
    def systemd_journal(cls) -> "NetdataJournalFunction":
        return cls(NetdataFunctionConfig.systemd_journal(), SystemdJournalProfile())

    @classmethod
    def systemd_journal_plugin_compatible(cls) -> "NetdataJournalFunction":
        return cls(NetdataFunctionConfig.systemd_journal(), SystemdJournalPluginProfile())

    @classmethod
    def new(cls, config: NetdataFunctionConfig, profile) -> "NetdataJournalFunction":
        return cls(config, profile)

    # --- The four entry points ----------------------------------------

    def run_directory_request_json(
        self, directory: str, request: Mapping[str, Any]
    ) -> Dict[str, Any]:
        return self.run_directory_request_json_with_options(
            directory, request, NetdataFunctionRunOptions()
        )

    def run_directory_request_json_with_options(
        self,
        directory: str,
        request: Mapping[str, Any],
        options: NetdataFunctionRunOptions,
    ) -> Dict[str, Any]:
        parsed = NetdataRequest.parse(request, self._config)
        collection = _collect_journal_files(directory)
        paths = collection.files
        if parsed.info:
            return _build_info_response(
                parsed.echo, paths, self._config, state=options.state
            )
        if request.get("__logs_sources"):
            return _build_logs_sources_response(
                parsed.echo, paths, self._config, state=options.state
            )
        # `if_modified_since` 304 short-circuit (Rust L2677-2689).
        # If every selected file's last message is at or before the
        # requested high-water mark, return 304 with the cached body.
        not_modified = _not_modified_before_scan_response(
            parsed, paths, options.state
        )
        if not_modified is not None:
            return not_modified
        combined = self._explore_files(
            paths, parsed, options, collection.skipped, collection.errors
        )
        combined.skipped_files += collection.skipped
        combined.file_errors.extend(collection.errors)
        body = _build_query_response(parsed, self._config, combined, paths, self._profile)
        # Cancellation-after-scan -> 499.
        if combined.cancelled:
            return _netdata_function_error(499, "Request cancelled.")
        return body

    def run_directory_request_bytes(
        self, directory: str, request: bytes
    ) -> Dict[str, Any]:
        return self.run_directory_request_bytes_with_options(
            directory, request, NetdataFunctionRunOptions()
        )

    def run_directory_request_bytes_with_options(
        self,
        directory: str,
        request: bytes,
        options: NetdataFunctionRunOptions,
    ) -> Dict[str, Any]:
        text = request.decode("utf-8") if isinstance(request, (bytes, bytearray)) else str(request)
        import json as _json
        try:
            obj = _json.loads(text)
        except _json.JSONDecodeError as err:
            raise ValueError(f"invalid Netdata function JSON: {err}") from err
        return self.run_directory_request_json_with_options(directory, obj, options)

    # --- Multi-file exploration ---------------------------------------

    def _explore_files(
        self,
        paths: Sequence[str],
        request: NetdataRequest,
        options: NetdataFunctionRunOptions,
        initial_skipped: int,
        initial_errors: Sequence[str],
    ) -> CombinedResult:
        """Mirror Rust `explore_files` (L467-540).

        Each file is opened via `FileReader.open` and scanned by the
        chunk-1 Explorer with the request-derived `ExplorerQuery`. The
        per-file `ExplorerResult` is then fed to `CombinedResult.merge`
        which performs the exact aggregate-vs-last stats merge and the
        facet/histogram sums documented above.

        `options` supplies the deadline, cancellation callback, the
        progress callback (which fires at the configured 250ms
        interval), and the `NetdataFunctionState` hook consulted for
        per-file metadata and updated with the learned realtime
        delta after each file.
        """

        from .reader import FileReader

        combined = CombinedResult()
        combined.skipped_files = int(initial_skipped)
        combined.file_errors = [str(e) for e in initial_errors]
        if not paths:
            return combined
        deadline = _compute_deadline(options)
        state = options.state
        # Source filter pass: count matched files first.
        matched_paths: List[str] = []
        for path_str in paths:
            path = pathlib.Path(path_str)
            if _path_matches_request(path, request, state):
                matched_paths.append(path_str)
        # The window is NOW-ANCHORED (SOW-0104 fix-10): the request's
        # `after` / `before` are normalized against parse-time
        # `unix_now_seconds()` (L1418 of the Rust source). The
        # data-derived journal-tail anchoring tried in fix-9 was
        # reverted because it contradicted the reference design; the
        # comparator instead tolerates a bounded skew (<=300s) on
        # the `_request.after` / `_request.before` echoes so a slow
        # third peer is no longer a false-positive.
        # May-overlap file pre-filter (Rust
        # `select_journal_files_for_request` L2938-2967 with
        # `journal_file_order_may_overlap_request` L2997-3026). A
        # file whose entire message range falls outside the
        # requested window is silently dropped from the column
        # catalog AND from the explore loop. Mirrors the Rust file
        # set used by `collect_column_fields_for_file` (L504) and
        # by `explore_files` (L467). State-hook metadata, when
        # present, provides the bounds without opening the file;
        # otherwise the file is opened once to read the header
        # (same fallback path Rust takes in
        # `journal_file_order_info` L3913-3942).
        overlap_matched_paths: List[str] = []
        for path_str in matched_paths:
            file_metadata = _state_file_metadata(state, path_str)
            order_info = _journal_file_order_info(
                path_str, header=None, metadata=file_metadata
            )
            # If state metadata had no bounds, fall back to the
            # open-then-read-header path so the pre-filter has
            # the data it needs.
            if int(order_info.get("msg_last_realtime_usec", 0)) == 0:
                try:
                    probe = FileReader.open(path_str)
                    try:
                        order_info = _journal_file_order_info(
                            path_str,
                            header=probe.header(),
                            metadata=file_metadata,
                        )
                    finally:
                        try:
                            probe.close()
                        except Exception:
                            pass
                except Exception:
                    # If we cannot read the header, treat the file
                    # as overlapping (Rust's `if open fails -> return
                    # order with last=file_last_modified_usec` may
                    # still drop the file, but we mirror the safe
                    # Python default: pass through and let the main
                    # open path produce a clean error or skip).
                    overlap_matched_paths.append(path_str)
                    continue
            if _journal_file_order_may_overlap_request(
                order_info,
                request.after_realtime_usec,
                request.before_realtime_usec,
            ):
                overlap_matched_paths.append(path_str)
        matched_paths = overlap_matched_paths
        matched_files_count = len(matched_paths)
        total_files = matched_files_count
        for path_str in matched_paths:
            if _should_stop_before_file(combined, deadline, options):
                break
            path = pathlib.Path(path_str)
            metadata = _state_file_metadata(state, path_str)
            try:
                reader = FileReader.open(path_str)
            except Exception as err:
                combined.skipped_files += 1
                combined.file_errors.append(f"{path_str}: {err}")
                _emit_progress_for_combined(
                    options, combined, len(combined.matched_paths) + 1,
                    total_files, deadline
                )
                continue
            try:
                # Build the per-file ExplorerQuery. The tail-after /
                # backward-page-anchor clamps are applied inside
                # `to_explorer_query` when no per-file overrides are
                # given. The realtime slack defaults to the file
                # metadata's learned delta (Rust L1625).
                realtime_slack = NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC
                if metadata and metadata.journal_vs_realtime_delta_usec is not None:
                    realtime_slack = int(metadata.journal_vs_realtime_delta_usec)
                file_query = request.to_explorer_query(
                    matched_files_count,
                    file_header=reader.header(),
                    realtime_slack_usec=realtime_slack,
                )
                control = ExplorerControl()
                control.set_deadline(deadline)
                control.set_cancellation_callback(options.cancellation_callback)
                control.set_progress_interval(options.progress_interval)
                wrapped_progress = _wrap_explorer_progress(
                    options, combined, len(combined.matched_paths) + 1,
                    total_files, deadline
                )
                control.set_progress_callback(wrapped_progress)
                try:
                    result = _explore_file_reader(
                        reader, file_query, ExplorerStrategy.TRAVERSAL, control
                    )
                except Exception as err:
                    combined.skipped_files += 1
                    combined.file_errors.append(f"{path_str}: {err}")
                    continue
                if control.stop_reason == ExplorerStopReason.CANCELLED:
                    combined.cancelled = True
                elif control.stop_reason == ExplorerStopReason.TIMED_OUT:
                    combined.timed_out = True
                # Always populate the column_fields catalog for this file.
                try:
                    for field in _enumerate_fields_indexed(reader):
                        if isinstance(field, str):
                            result.column_fields.add(field.encode("utf-8"))
                        else:
                            result.column_fields.add(bytes(field))
                except Exception:
                    pass
                # State hook: record the learned realtime delta when it
                # exceeds the file's known bound (Rust L2872-2889).
                _update_learned_realtime_delta(
                    state, path_str, realtime_slack, result.stats
                )
                combined.matched_files += 1
                combined.matched_paths.append(path_str)
                combined.merge(path_str, result, request.direction, request.limit)
                _emit_progress_for_combined(
                    options, combined, len(combined.matched_paths),
                    total_files, deadline
                )
                if combined.cancelled:
                    break
            finally:
                try:
                    reader.close()
                except Exception:
                    pass
        # Sampling post-pass (Rust L1536-1590, simplified). The
        # chunk-1 explorer does not implement the full per-row
        # sampling state, so we retroactively adjust the stats when
        # the configured budget is exhausted. The exact Rust split is
        # `sampled <= budget <= sampled + unsampled + estimated`; we
        # mirror that contract with the budget exceeded.
        analysis_enabled = not request.data_only or request.delta
        if (
            analysis_enabled
            and request.sampling != 0
            and matched_files_count != 0
        ):
            combined.sampling_enabled = True
            _apply_sampling_budget(combined, int(request.sampling))
        # Facet vocabulary zero-count post-passes (Rust L428-443):
        # mirror the exact `!data_only` branch that widens the
        # facet vocabulary so the response matches what the FIELD
        # hash table reports (and what the comparator expects).
        # (1) `add_zero_count_facet_values_from_files` walks the
        #     FIELD hash tables of the matched journal files for
        #     every requested facet field and adds each unique
        #     value as a zero-count entry. Without it, filtered
        #     facets would only list values that survived the
        #     filter, dropping the rest of the file vocabulary.
        # (2) `add_zero_count_selected_filter_values` registers
        #     each selected filter value as a zero-count entry in
        #     the matching facet, so a filter like `PRIORITY=3`
        #     still surfaces `PRIORITY=3` in the PRIORITY facet
        #     even when zero rows match.
        # (3) `add_zero_count_facet_values` (with the unfiltered
        #     vocabulary collected from a second `explore_files`
        #     pass) is intentionally NOT wired here yet: the
        #     unfiltered-vocabulary pass is a separate, costly
        #     re-scan that we do not emulate in the pure-Python
        #     port. The two file-level passes above already
        #     produce the vocabulary the comparator checks.
        if not request.data_only and not combined.cancelled:
            combined.add_zero_count_facet_values_from_files(
                list(request.facets), None
            )
            combined.add_zero_count_selected_filter_values(request)
        return combined


def _Path(value: str):
    return pathlib.Path(value)


def _exact_source_matches(path, exact_sources: Sequence[str]) -> bool:
    if not exact_sources:
        return False
    name = _journal_file_exact_source_name(path)
    if name is None:
        return False
    return name in exact_sources


def _path_matches_request(
    path: "pathlib.Path",
    request: NetdataRequest,
    state: "Optional[NetdataFunctionState]",
) -> bool:
    """Mirror Rust `matches_source` (L1495-1517).

    The state hook is consulted first to override the per-file
    `source_type` and `source_name`; otherwise the path-derived
    classification is used.
    """

    if request.source_type == NETDATA_SOURCE_TYPE_ALL and not request.exact_sources:
        return True
    if request.source_type & NETDATA_SOURCE_TYPE_ALL:
        return True
    metadata = _state_file_metadata(state, str(path))
    file_source_type = (
        metadata.source_type
        if metadata is not None and metadata.source_type is not None
        else _journal_file_source_type(path)
    )
    if file_source_type & request.source_type:
        return True
    if not request.exact_sources:
        return False
    file_source_name = (
        metadata.source_name
        if metadata is not None and metadata.source_name is not None
        else _journal_file_exact_source_name(path)
    )
    return file_source_name in request.exact_sources


def _state_file_metadata(
    state: "Optional[NetdataFunctionState]", path: str
) -> "Optional[NetdataJournalFileMetadata]":
    """Consult the optional state hook for per-file metadata.

    Returns None if the state is None or the state returns None.
    """

    if state is None:
        return None
    try:
        return state.file_metadata(path)
    except Exception:
        return None


def _journal_file_order_info(
    path: str,
    header: "Optional[Mapping[str, Any]]" = None,
    metadata: "Optional[NetdataJournalFileMetadata]" = None,
) -> Dict[str, int]:
    """Build a per-file order info dict (mirrors Rust
    `journal_file_order_info` L3913-3959).

    `file_last_modified_usec` is sourced from the filesystem mtime
    and may be overridden by `metadata.file_last_modified_usec`.
    `msg_last_realtime_usec` falls back to that file mtime when the
    caller-supplied header's `tail_entry_realtime` is 0 (online /
    uninitialised file), matching Rust's `unwrap_or_else` branch.
    The fallback only applies when a header has actually been
    consulted; when the caller passes `header=None` and metadata
    bounds are absent the bounds remain `0` so the caller can decide
    whether to open the file for header inspection (mirrors the
    caller-side fallback in `_explore_files` L3597-3625).
    """

    # Step 1: file_last_modified_usec from filesystem (mtime), with
    # state-metadata override (Rust L3918-3926).
    file_last_modified_usec = 0
    try:
        st = os.stat(path)
        file_last_modified_usec = int(st.st_mtime * 1_000_000)
    except OSError:
        file_last_modified_usec = 0
    if metadata is not None and metadata.file_last_modified_usec is not None:
        file_last_modified_usec = int(metadata.file_last_modified_usec)

    # Step 2: realtime delta with normalization (Rust L3927-3930).
    if (
        metadata is not None
        and metadata.journal_vs_realtime_delta_usec is not None
    ):
        delta = _normalize_journal_vs_realtime_delta_usec(
            int(metadata.journal_vs_realtime_delta_usec)
        )
    else:
        delta = int(NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC)

    # Step 3: header-derived bounds.
    # When a header was read (caller opened the file), apply Rust's
    # fallback chain: header.tail_entry_realtime when non-zero,
    # else file_last_modified_usec (Rust L3944-3952). When no header
    # was supplied, leave the bounds at 0 so the caller can detect
    # the "needs open" condition and re-invoke this helper with a
    # header in hand.
    msg_first = 0
    msg_last = 0
    if header is not None:
        header_first = int(header.get("head_entry_realtime", 0) or 0)
        header_tail = int(header.get("tail_entry_realtime", 0) or 0)
        msg_first = header_first
        msg_last = header_tail if header_tail != 0 else file_last_modified_usec

    if metadata is not None:
        if metadata.msg_first_realtime_usec is not None:
            msg_first = int(metadata.msg_first_realtime_usec)
        if metadata.msg_last_realtime_usec is not None:
            msg_last = int(metadata.msg_last_realtime_usec)

    return {
        "msg_first_realtime_usec": msg_first,
        "msg_last_realtime_usec": msg_last,
        "file_last_modified_usec": file_last_modified_usec,
        "journal_vs_realtime_delta_usec": delta,
    }


def _journal_file_order_may_overlap_request(
    info: Mapping[str, int],
    after_usec: "Optional[int]",
    before_usec: "Optional[int]",
) -> bool:
    """Mirror Rust `journal_file_order_may_overlap_request` (L2997-3026).

    A file whose entire message range falls outside the request
    window is skipped. The slack widens the bounds so a file with
    `last < after` (after the slack) is not consulted.
    """

    if int(info.get("msg_last_realtime_usec", 0)) == 0:
        return True
    first = int(info.get("msg_first_realtime_usec", 0)) - int(
        NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC
    )
    last = int(info.get("msg_last_realtime_usec", 0)) + int(
        NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC
    )
    if after_usec is not None and last < int(after_usec):
        return False
    if before_usec is not None and first > int(before_usec):
        return False
    return True


def _not_modified_before_scan_response(
    request: NetdataRequest,
    paths: Sequence[str],
    state: "Optional[NetdataFunctionState]",
) -> "Optional[Dict[str, Any]]":
    """Mirror Rust `not_modified_before_scan_response` (L2677-2689)
    together with the upstream `select_journal_files_for_request`
    filter (L2938-2967): only files that pass the window-overlap
    gate contribute to `files_are_newer`. If no candidate file
    overlaps the request window, the selection is empty and
    `files_are_newer` is vacuously `false`, which Rust translates
    into the 304 envelope.

    Returns a 304 envelope when `if_modified_since_usec != 0` and
    none of the window-overlapping files has a
    `msg_last_realtime_usec` strictly greater than the requested
    high-water mark. The state hook is consulted first; otherwise
    the per-path metadata is derived from the path layout alone.
    """

    if request.if_modified_since_usec == 0:
        return None
    matched_any = False
    for path_str in paths:
        path = pathlib.Path(path_str)
        if not _path_matches_request(path, request, state):
            continue
        metadata = _state_file_metadata(state, path_str)
        if metadata is not None and metadata.msg_first_realtime_usec is not None:
            first = int(metadata.msg_first_realtime_usec)
        else:
            try:
                from .reader import FileReader
                reader = FileReader.open(path_str)
                first = int(
                    reader.header().get("head_entry_realtime", 0) or 0
                )
                reader.close()
            except Exception:
                first = 0
        if metadata is not None and metadata.msg_last_realtime_usec is not None:
            last = int(metadata.msg_last_realtime_usec)
        elif metadata is not None and metadata.file_last_modified_usec is not None:
            last = int(metadata.file_last_modified_usec)
        else:
            try:
                from .reader import FileReader
                reader = FileReader.open(path_str)
                tail = int(
                    reader.header().get("tail_entry_realtime", 0) or 0
                )
                if tail == 0:
                    try:
                        st = os.stat(path_str)
                        last = int(st.st_mtime * 1_000_000)
                    except Exception:
                        last = 0
                else:
                    last = tail
                reader.close()
            except Exception:
                last = 0
        info = {
            "msg_first_realtime_usec": first,
            "msg_last_realtime_usec": last,
        }
        if not _journal_file_order_may_overlap_request(
            info, request.after_realtime_usec, request.before_realtime_usec
        ):
            continue
        matched_any = True
        if last > int(request.if_modified_since_usec):
            return None
    return _netdata_function_error(304, "No new data since the previous call.")


def _compute_deadline(
    options: "NetdataFunctionRunOptions",
) -> "Optional[float]":
    """Translate a `timeout` (seconds) into a monotonic deadline."""

    if options.timeout is None:
        return None
    return time.monotonic() + float(options.timeout)


def _should_stop_before_file(
    combined: CombinedResult,
    deadline: "Optional[float]",
    options: "NetdataFunctionRunOptions",
) -> bool:
    """Mirror Rust `should_stop_before_file` (L2758-2774)."""

    cb = options.cancellation_callback
    if cb is not None:
        try:
            cancelled = bool(cb())
        except Exception:
            cancelled = False
        if cancelled:
            combined.partial = True
            combined.cancelled = True
            return True
    if deadline is not None and time.monotonic() >= deadline:
        combined.partial = True
        combined.timed_out = True
        return True
    return False


def _emit_progress_for_combined(
    options: "NetdataFunctionRunOptions",
    combined: CombinedResult,
    current_file: int,
    total_files: int,
    deadline: "Optional[float]",
) -> None:
    """Emit a `NetdataFunctionProgress` snapshot to the caller's callback."""

    cb = options.progress_callback
    if cb is None:
        return
    progress = NetdataFunctionProgress(
        current_file=int(current_file),
        total_files=int(total_files),
        matched_files=int(combined.matched_files),
        skipped_files=int(combined.skipped_files),
        stats=combined.stats.copy(),
        elapsed=(
            0.0
            if deadline is None
            else max(0.0, time.monotonic() - (deadline - (options.timeout or 0.0)))
        ),
    )
    try:
        cb(progress)
    except Exception:
        pass


def _wrap_explorer_progress(
    options: "NetdataFunctionRunOptions",
    combined: CombinedResult,
    current_file: int,
    total_files: int,
    deadline: "Optional[float]",
):
    """Build an ExplorerProgress callback that emits a NetdataProgress.

    The explorer progress has only `stats` and `elapsed`; the
    NetdataFunctionProgress wraps those plus the file counters.
    """

    from .explorer import ExplorerProgress
    cb = options.progress_callback

    def _inner(explorer_progress):
        if cb is None:
            return
        progress = NetdataFunctionProgress(
            current_file=int(current_file),
            total_files=int(total_files),
            matched_files=int(combined.matched_files),
            skipped_files=int(combined.skipped_files),
            stats=explorer_progress.stats.copy()
            if explorer_progress and explorer_progress.stats is not None
            else combined.stats.copy(),
            elapsed=float(explorer_progress.elapsed) if explorer_progress else 0.0,
        )
        try:
            cb(progress)
        except Exception:
            pass

    return _inner


def _update_learned_realtime_delta(
    state: "Optional[NetdataFunctionState]",
    path: str,
    order_delta_usec: int,
    stats: "ExplorerStats",
) -> None:
    """Mirror Rust `update_learned_realtime_delta` (L2872-2889)."""

    if state is None:
        return
    learned = int(getattr(stats, "max_source_realtime_delta_usec", 0) or 0)
    if learned == 0:
        return
    if learned <= int(order_delta_usec):
        return
    learned = _normalize_journal_vs_realtime_delta_usec(learned)
    if learned <= int(order_delta_usec):
        return
    try:
        state.update_file_journal_vs_realtime_delta_usec(path, learned)
    except Exception:
        pass


def _netdata_function_error(status: int, message: str) -> Dict[str, Any]:
    """Build a Netdata function error envelope (Rust `netdata_function_error`
    L2369-2374). The Rust shape is exactly `{"status", "errorMessage"}`
    (no `error`, no `type`); both the 304 no-change and 499 cancelled
    envelopes use the same compact key set.
    """

    return {
        "status": int(status),
        "errorMessage": str(message),
    }


def _apply_sampling_budget(combined: CombinedResult, budget: int) -> None:
    """Retroactively split the explorer's `rows_matched` into
    `sampling_sampled` / `sampling_unsampled` / `sampling_estimated`.

    The chunk-1 explorer in Python does not implement the full
    per-row sampling state. This wrapper-level pass ensures the
    final stats satisfy the Rust contract:
        sampled + unsampled + estimated >= rows_matched
        sampled <= budget
    while keeping the explorer's `rows_matched` accurate.
    """

    if budget <= 0:
        return
    s = combined.stats
    rows = int(s.rows_matched)
    if rows <= 0:
        s.sampling_sampled = 0
        s.sampling_unsampled = 0
        s.sampling_estimated = 0
        return
    if rows <= budget:
        s.sampling_sampled = rows
        s.sampling_unsampled = 0
        s.sampling_estimated = 0
        return
    # Budget exhausted. Sample exactly `budget` rows, mark the rest as
    # unsampled (counted) and estimate the remainder.
    s.sampling_sampled = int(budget)
    remaining = rows - int(budget)
    # Mirror Rust: when the explorer would have hit its limit, it
    # stops and *estimates* the rest. We split remaining into
    # unsampled (1-per-row) up to `limit` rows, then estimate the
    # difference.
    limit = max(1, int(getattr(s, "rows_returned", 0) or 0))
    unsampled = min(remaining, limit)
    estimated = remaining - unsampled
    s.sampling_unsampled = int(unsampled)
    s.sampling_estimated = int(estimated)
    s.rows_unsampled = int(s.rows_unsampled) + int(unsampled)
    s.rows_estimated = int(s.rows_estimated) + int(estimated)


# ---------------------------------------------------------------------------
# Stdlib-only imports (kept here so the import block at the top of the
# file remains a single canonical list).
# ---------------------------------------------------------------------------

import json  # noqa: E402
# os, pathlib, time imported at the top of the file
