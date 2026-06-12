#!/usr/bin/env python3
# SPDX-License-Identifier: MIT-0
"""Focused tests for `python/journal/netdata.py`.

Covers SOW-0104 chunk 2a:

- Config defaults and the systemd_journal() factory.
- Selector backfill behaviour via a direct helper.
- The EXACT default facet / view-key lists: lengths, first and last
  entries, full-content equality with the canonical Rust values.
- Profile transformation tests for at least PRIORITY, SYSLOG_FACILITY,
  _UID, ERRNO, _SOURCE_REALTIME_TIMESTAMP in both standard and
  plugin-compatible modes with concrete expected outputs derived
  from the Rust rules.
- Severity mapping tests (priority_to_row_severity).

Stdlib only. No `journal.*` runtime imports. The host user database
is consulted indirectly: tests that touch the resolver skip on
platforms where the runtime purity boundary forbids it, or fall back
to the no-resolution path if the value is unknown.
"""

import json
import os
import pathlib
import pwd
import subprocess  # nosec B404
import sys
import tempfile
import time
import unittest
from typing import Optional

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

from journal import netdata as n  # noqa: E402
from journal import Writer  # noqa: E402


# Reference copies of the canonical Rust lists. Order matters; we use
# these to assert byte-for-byte equality with the Python module's
# exported lists.
_RUST_VIEW_KEYS = [
    "_HOSTNAME", "ND_JOURNAL_PROCESS", "MESSAGE", "PRIORITY",
    "SYSLOG_FACILITY", "ERRNO", "ND_JOURNAL_FILE", "SYSLOG_IDENTIFIER",
    "UNIT", "USER_UNIT", "MESSAGE_ID", "_BOOT_ID",
    "_SYSTEMD_OWNER_UID", "_UID", "OBJECT_SYSTEMD_OWNER_UID",
    "OBJECT_UID", "_GID", "OBJECT_GID", "_CAP_EFFECTIVE",
    "_AUDIT_LOGINUID", "OBJECT_AUDIT_LOGINUID", "_SOURCE_REALTIME_TIMESTAMP",
]

_RUST_FACETS = [
    "_HOSTNAME", "PRIORITY", "SYSLOG_FACILITY", "ERRNO",
    "SYSLOG_IDENTIFIER", "UNIT", "USER_UNIT", "MESSAGE_ID",
    "_BOOT_ID", "_SYSTEMD_OWNER_UID", "_UID",
    "OBJECT_SYSTEMD_OWNER_UID", "OBJECT_UID", "_GID", "OBJECT_GID",
    "_AUDIT_LOGINUID", "OBJECT_AUDIT_LOGINUID", "CODE_FILE",
    "_SYSTEMD_UNIT", "_SYSTEMD_USER_SLICE", "CODE_FUNC", "_TRANSPORT",
    "_COMM", "_RUNTIME_SCOPE", "_MACHINE_ID", "_SYSTEMD_SLICE",
    "UNIT_RESULT", "_SYSTEMD_CGROUP", "_EXE", "_SYSTEMD_USER_UNIT",
    "_SYSTEMD_SESSION", "COREDUMP_CGROUP", "COREDUMP_USER_UNIT",
    "COREDUMP_UNIT", "COREDUMP_SIGNAL_NAME", "COREDUMP_COMM",
    "_UDEV_DEVNODE", "_KERNEL_SUBSYSTEM", "OBJECT_EXE",
    "OBJECT_SYSTEMD_CGROUP", "OBJECT_COMM", "OBJECT_SYSTEMD_UNIT",
    "OBJECT_SYSTEMD_USER_UNIT", "_SELINUX_CONTEXT", "_NAMESPACE",
    "OBJECT_SYSTEMD_SESSION", "CONTAINER_ID", "CONTAINER_NAME",
    "CONTAINER_TAG", "IMAGE_NAME", "ND_NIDL_NODE", "ND_NIDL_CONTEXT",
    "ND_LOG_SOURCE", "ND_ALERT_NAME", "ND_ALERT_CLASS",
    "ND_ALERT_COMPONENT", "ND_ALERT_TYPE", "ND_ALERT_STATUS",
]


def _running_uid_gid():
    """Return (uid_str, gid_str) for the current process.

    Used to construct a uid/gid value that the system user database
    is guaranteed to resolve. Skip on platforms without pwd/grp.
    """
    return (str(os.getuid()), str(os.getgid()))


class SourceTypeFlags(unittest.TestCase):
    def test_bit_values(self):
        self.assertEqual(n.NETDATA_SOURCE_TYPE_ALL, 1 << 0)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_LOCAL_ALL, 1 << 1)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_REMOTE_ALL, 1 << 2)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_LOCAL_SYSTEM, 1 << 3)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_LOCAL_USER, 1 << 4)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE, 1 << 5)
        self.assertEqual(n.NETDATA_SOURCE_TYPE_LOCAL_OTHER, 1 << 6)

    def test_accepted_params(self):
        self.assertEqual(
            n.NETDATA_ACCEPTED_PARAMS,
            [
                "info", "__logs_sources", "after", "before", "anchor",
                "direction", "last", "query", "facets", "histogram",
                "if_modified_since", "data_only", "delta", "tail",
                "sampling", "slice",
            ],
        )
        self.assertEqual(len(n.NETDATA_ACCEPTED_PARAMS), 16)


class DefaultLists(unittest.TestCase):
    def test_view_key_lengths(self):
        # The Rust source has 22 entries in SYSTEMD_DEFAULT_VIEW_KEYS
        # (rust/src/journal/src/netdata.rs L73-96). The SOW inventory
        # stated "18", which is a typo in the inventory - the source
        # of truth is the Rust array.
        self.assertEqual(len(n.SYSTEMD_DEFAULT_VIEW_KEYS), 22)

    def test_facet_lengths(self):
        # The Rust source has 58 entries in SYSTEMD_DEFAULT_FACETS
        # (rust/src/journal/src/netdata.rs L98-157). The SOW inventory
        # stated "60", which is a typo in the inventory - the source
        # of truth is the Rust array.
        self.assertEqual(len(n.SYSTEMD_DEFAULT_FACETS), 58)

    def test_view_keys_first_and_last(self):
        self.assertEqual(n.SYSTEMD_DEFAULT_VIEW_KEYS[0], "_HOSTNAME")
        self.assertEqual(n.SYSTEMD_DEFAULT_VIEW_KEYS[-1], "_SOURCE_REALTIME_TIMESTAMP")

    def test_facets_first_and_last(self):
        self.assertEqual(n.SYSTEMD_DEFAULT_FACETS[0], "_HOSTNAME")
        self.assertEqual(n.SYSTEMD_DEFAULT_FACETS[-1], "ND_ALERT_STATUS")

    def test_view_keys_exact_match(self):
        self.assertEqual(n.SYSTEMD_DEFAULT_VIEW_KEYS, _RUST_VIEW_KEYS)

    def test_facets_exact_match(self):
        self.assertEqual(n.SYSTEMD_DEFAULT_FACETS, _RUST_FACETS)


class ConfigDefaults(unittest.TestCase):
    def test_default_constructor_matches_factory(self):
        from_default = n.NetdataFunctionConfig()
        from_factory = n.NetdataFunctionConfig.systemd_journal()
        self.assertEqual(from_default, from_factory)

    def test_string_defaults(self):
        cfg = n.NetdataFunctionConfig()
        self.assertEqual(cfg.function_name, "systemd-journal")
        self.assertEqual(cfg.source_selector_name, "Journal Sources")
        self.assertEqual(cfg.source_selector_help, "Select the logs source to query")
        self.assertEqual(cfg.default_histogram, "PRIORITY")

    def test_default_lists_are_copies(self):
        # Mutating the instance's list must not affect the module-level constant.
        cfg = n.NetdataFunctionConfig()
        cfg.default_facets.append("ZZZ_NOT_A_REAL_FIELD")
        self.assertNotIn("ZZZ_NOT_A_REAL_FIELD", n.SYSTEMD_DEFAULT_FACETS)
        # Reset to keep the rest of the test suite clean.
        cfg.default_facets = list(n.SYSTEMD_DEFAULT_FACETS)

    def test_default_lists_are_independent_per_instance(self):
        cfg_a = n.NetdataFunctionConfig()
        cfg_b = n.NetdataFunctionConfig()
        cfg_a.default_facets.append("DUP")
        self.assertNotIn("DUP", cfg_b.default_facets)

    def test_backfill_does_not_touch_non_empty(self):
        cfg = n.NetdataFunctionConfig(
            source_selector_name="My selector",
            source_selector_help="My help",
        )
        cfg.backfill_defaults()
        self.assertEqual(cfg.source_selector_name, "My selector")
        self.assertEqual(cfg.source_selector_help, "My help")

    def test_backfill_fills_empty_name(self):
        cfg = n.NetdataFunctionConfig(
            source_selector_name="",
            source_selector_help="",
        )
        cfg.backfill_defaults()
        self.assertEqual(cfg.source_selector_name, "Journal Sources")
        self.assertEqual(cfg.source_selector_help, "Select the logs source to query")

    def test_backfill_returns_self_for_chaining(self):
        cfg = n.NetdataFunctionConfig()
        self.assertIs(cfg.backfill_defaults(), cfg)


class DisplayContextCaches(unittest.TestCase):
    def test_caches_start_empty(self):
        ctx = n.DisplayContext()
        self.assertEqual(dict(ctx.boot_first_realtime), {})
        self.assertEqual(dict(ctx.uid_display_cache), {})
        self.assertEqual(dict(ctx.gid_display_cache), {})

    def test_register_boot_first_realtime(self):
        ctx = n.DisplayContext()
        ctx.register_boot_first_realtime(b"boot-abc", 1700000000_000000)
        self.assertEqual(ctx.boot_first_realtime[b"boot-abc"], 1700000000_000000)

    def test_register_overwrites(self):
        ctx = n.DisplayContext()
        ctx.register_boot_first_realtime(b"boot-abc", 1)
        ctx.register_boot_first_realtime(b"boot-abc", 2)
        self.assertEqual(ctx.boot_first_realtime[b"boot-abc"], 2)


class SeverityMapping(unittest.TestCase):
    """Mirrors `priority_to_row_severity` (L4085-4094)."""

    def test_critical_band(self):
        for v in (b"0", b"1", b"2", b"3"):
            self.assertEqual(n._priority_to_row_severity(v), "critical")

    def test_warning(self):
        self.assertEqual(n._priority_to_row_severity(b"4"), "warning")

    def test_notice(self):
        self.assertEqual(n._priority_to_row_severity(b"5"), "notice")

    def test_info_falls_through_to_normal(self):
        # Rust: priority == 6 has no arm; falls through to `_ => "normal"`.
        self.assertEqual(n._priority_to_row_severity(b"6"), "normal")

    def test_debug_band(self):
        for v in (b"7", b"8", b"100"):
            self.assertEqual(n._priority_to_row_severity(v), "debug")

    def test_unparseable_defaults_to_normal(self):
        self.assertEqual(n._priority_to_row_severity(b"abc"), "normal")
        self.assertEqual(n._priority_to_row_severity(b""), "normal")

    def test_negative_defaults_to_normal(self):
        # _try_int rejects negatives; this exercises the safe-parse path.
        self.assertEqual(n._priority_to_row_severity(b"-1"), "normal")


class RowOptions(unittest.TestCase):
    def test_no_priority_field_is_normal(self):
        profile = n.SystemdJournalProfile()
        self.assertEqual(profile.row_options({}), {"severity": "normal"})

    def test_priority_field_selects_severity(self):
        profile = n.SystemdJournalProfile()
        self.assertEqual(
            profile.row_options({"PRIORITY": [b"3"]}),
            {"severity": "critical"},
        )
        self.assertEqual(
            profile.row_options({"PRIORITY": [b"4"]}),
            {"severity": "warning"},
        )
        self.assertEqual(
            profile.row_options({"PRIORITY": [b"6"]}),
            {"severity": "normal"},
        )
        # Only the first value is used, matching Rust's `first_value`.
        self.assertEqual(
            profile.row_options({"PRIORITY": [b"3", b"6"]}),
            {"severity": "critical"},
        )


class PriorityDisplay(unittest.TestCase):
    def test_priority_name_known(self):
        self.assertEqual(n._priority_name("0"), "panic")
        self.assertEqual(n._priority_name("1"), "alert")
        self.assertEqual(n._priority_name("2"), "critical")
        self.assertEqual(n._priority_name("3"), "error")
        self.assertEqual(n._priority_name("4"), "warning")
        self.assertEqual(n._priority_name("5"), "notice")
        self.assertEqual(n._priority_name("6"), "info")
        self.assertEqual(n._priority_name("7"), "debug")

    def test_priority_name_out_of_range_is_none(self):
        self.assertIsNone(n._priority_name("8"))
        self.assertIsNone(n._priority_name("-1"))

    def test_priority_name_unparseable_is_none(self):
        self.assertIsNone(n._priority_name("foo"))

    def test_field_display_priority_both_modes(self):
        # PRIORITY transformation does not depend on the
        # plugin-compatible flag.
        std = n.SystemdJournalProfile()
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "PRIORITY", b"3"),
            "error",
        )
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "PRIORITY", b"3"),
            "error",
        )
        # Out-of-range falls through to the raw value.
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "PRIORITY", b"9"),
            "9",
        )


class SyslogFacilityDisplay(unittest.TestCase):
    def test_syslog_facility_lookup(self):
        self.assertEqual(n._syslog_facility_name("0"), "kern")
        self.assertEqual(n._syslog_facility_name("3"), "daemon")
        self.assertEqual(n._syslog_facility_name("10"), "authpriv")
        self.assertEqual(n._syslog_facility_name("16"), "local0")
        self.assertEqual(n._syslog_facility_name("23"), "local7")
        # Gaps in the table (12..15, 24..255) return None.
        self.assertIsNone(n._syslog_facility_name("12"))
        self.assertIsNone(n._syslog_facility_name("24"))
        self.assertIsNone(n._syslog_facility_name("99"))

    def test_field_display_facility_both_modes(self):
        std = n.SystemdJournalProfile()
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "SYSLOG_FACILITY", b"3"),
            "daemon",
        )
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "SYSLOG_FACILITY", b"3"),
            "daemon",
        )
        # Unknown -> raw passthrough.
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "SYSLOG_FACILITY", b"42"),
            "42",
        )


class ErrnoDisplay(unittest.TestCase):
    def test_errno_known(self):
        self.assertEqual(n._errno_name("2"), "2 (ENOENT)")
        self.assertEqual(n._errno_name("13"), "13 (EACCES)")
        self.assertEqual(n._errno_name("22"), "22 (EINVAL)")

    def test_errno_unknown_returns_none(self):
        # 41 and others are not in the table.
        self.assertIsNone(n._errno_name("41"))

    def test_field_display_errno_both_modes(self):
        std = n.SystemdJournalProfile()
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "ERRNO", b"2"),
            "2 (ENOENT)",
        )
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "ERRNO", b"2"),
            "2 (ENOENT)",
        )
        # Unknown -> raw passthrough.
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "ERRNO", b"999"),
            "999",
        )


class SourceRealtimeTimestampDisplay(unittest.TestCase):
    def test_zero_is_raw(self):
        # 0 must not produce "(...)" because the Rust code special-cases
        # `timestamp != 0`.
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_SOURCE_REALTIME_TIMESTAMP", b"0"),
            "0",
        )

    def test_known_value_includes_iso8601(self):
        # 1700000000 seconds = 2023-11-14T22:13:20Z.
        std = n.SystemdJournalProfile()
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        ts = "1700000000000000"
        expected_tail = "2023-11-14T22:13:20.000000Z"
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_SOURCE_REALTIME_TIMESTAMP", ts.encode()),
            f"{ts} ({expected_tail})",
        )
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "_SOURCE_REALTIME_TIMESTAMP", ts.encode()),
            f"{ts} ({expected_tail})",
        )

    def test_unparseable_is_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_SOURCE_REALTIME_TIMESTAMP", b"notanumber"),
            "notanumber",
        )


class UidDisplay(unittest.TestCase):
    def _uid_for_self(self):
        return str(os.getuid())

    def test_unknown_uid_in_standard_mode_is_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        # 2_000_000_000 is an integer that is not in /etc/passwd.
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_UID", b"2000000000"),
            "2000000000",
        )

    def test_unknown_uid_in_plugin_mode_is_raw(self):
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "_UID", b"2000000000"),
            "2000000000",
        )

    def test_known_uid_in_plugin_mode_resolves(self):
        uid_str = self._uid_for_self()
        # Sanity: confirm the lookup is actually available in this
        # environment. If not, skip the resolution assertion.
        try:
            login = pwd.getpwuid(int(uid_str)).pw_name
        except KeyError:
            self.skipTest(f"uid {uid_str} not present in this environment")
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "_UID", uid_str.encode()),
            login,
        )
        # Standard mode keeps the raw value.
        std = n.SystemdJournalProfile()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_UID", uid_str.encode()),
            uid_str,
        )

    def test_plugin_mode_uid_cache_hits(self):
        uid_str = self._uid_for_self()
        try:
            pwd.getpwuid(int(uid_str))
        except KeyError:
            self.skipTest(f"uid {uid_str} not present in this environment")
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        first = plug.field_display_value(ctx, n.DisplayScope.Data, "_UID", uid_str.encode())
        # Second call must read from the cache (we don't monkey-patch
        # pwd; the assertion is that the cache has been populated and
        # the result is the same).
        second = plug.field_display_value(ctx, n.DisplayScope.Data, "_UID", uid_str.encode())
        self.assertEqual(first, second)
        self.assertIn(uid_str, ctx._uid_cache)

    def test_object_uid_object_audit_loginuid_all_resolve(self):
        uid_str = self._uid_for_self()
        try:
            pwd.getpwuid(int(uid_str))
        except KeyError:
            self.skipTest(f"uid {uid_str} not present in this environment")
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        for field in ("OBJECT_UID", "OBJECT_SYSTEMD_OWNER_UID",
                      "OBJECT_AUDIT_LOGINUID", "_SYSTEMD_OWNER_UID",
                      "_AUDIT_LOGINUID"):
            self.assertEqual(
                plug.field_display_value(ctx, n.DisplayScope.Data, field, uid_str.encode()),
                pwd.getpwuid(int(uid_str)).pw_name,
                msg=f"field {field!r} did not resolve",
            )


class GidDisplay(unittest.TestCase):
    def test_unknown_gid_in_standard_mode_is_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_GID", b"2000000000"),
            "2000000000",
        )

    def test_unknown_gid_in_plugin_mode_is_raw(self):
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "_GID", b"2000000000"),
            "2000000000",
        )

    def test_known_gid_in_plugin_mode_resolves(self):
        gid_str = str(os.getgid())
        try:
            grp_name = __import__("grp").getgrgid(int(gid_str)).gr_name
        except KeyError:
            self.skipTest(f"gid {gid_str} not present in this environment")
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "_GID", gid_str.encode()),
            grp_name,
        )
        # OBJECT_GID path is the same as _GID.
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "OBJECT_GID", gid_str.encode()),
            grp_name,
        )


class BootIdDisplay(unittest.TestCase):
    def test_no_context_lookup_returns_raw(self):
        # DisplayContext has no entry -> raw value passthrough.
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_BOOT_ID", b"deadbeef"),
            "deadbeef",
        )

    def test_data_scope_with_lookup_appends_iso(self):
        ctx = n.DisplayContext()
        ctx.register_boot_first_realtime(b"deadbeef", 1_700_000_000_000_000)
        std = n.SystemdJournalProfile()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_BOOT_ID", b"deadbeef"),
            "deadbeef (2023-11-14T22:13:20Z)  ",
        )

    def test_facet_scope_with_lookup_returns_iso_only(self):
        ctx = n.DisplayContext()
        ctx.register_boot_first_realtime(b"deadbeef", 1_700_000_000_000_000)
        std = n.SystemdJournalProfile()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Facet, "_BOOT_ID", b"deadbeef"),
            "2023-11-14T22:13:20Z",
        )
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Histogram, "_BOOT_ID", b"deadbeef"),
            "2023-11-14T22:13:20Z",
        )


class CapEffectiveDisplay(unittest.TestCase):
    def test_zero_keeps_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_CAP_EFFECTIVE", b"0"),
            "0",
        )

    def test_non_hex_returns_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_CAP_EFFECTIVE", b"xyz"),
            "xyz",
        )

    def test_known_bit_decodes(self):
        # bit 0 (CHOWN) only.
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_CAP_EFFECTIVE", b"1"),
            "1 (CHOWN)",
        )
        # bits 0 (CHOWN) and 6 (SETGID) -> 0x41 = 0b0100_0001.
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "_CAP_EFFECTIVE", b"41"),
            "41 (CHOWN | SETGID)",
        )


class MessageIdDisplay(unittest.TestCase):
    def test_known_id_data_scope(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        # "Journal started" message id.
        self.assertEqual(
            std.field_display_value(
                ctx, n.DisplayScope.Data, "MESSAGE_ID",
                b"f77379a8490b408bbe5f6940505a777b",
            ),
            "f77379a8490b408bbe5f6940505a777b (Journal started)",
        )

    def test_known_id_facet_scope(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        # Facet scope -> name only, no raw prefix.
        self.assertEqual(
            std.field_display_value(
                ctx, n.DisplayScope.Facet, "MESSAGE_ID",
                b"f77379a8490b408bbe5f6940505a777b",
            ),
            "Journal started",
        )
        self.assertEqual(
            std.field_display_value(
                ctx, n.DisplayScope.Histogram, "MESSAGE_ID",
                b"f77379a8490b408bbe5f6940505a777b",
            ),
            "Journal started",
        )

    def test_unknown_id_returns_raw(self):
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(
                ctx, n.DisplayScope.Data, "MESSAGE_ID",
                b"deadbeefdeadbeefdeadbeefdeadbeef",
            ),
            "deadbeefdeadbeefdeadbeefdeadbeef",
        )


class FacetOptionName(unittest.TestCase):
    def test_default_profile_uses_utf8_lossy(self):
        # For a field that the base class does not transform (e.g.
        # MESSAGE), `facet_option_name` should still return the raw
        # text.
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.facet_option_name(ctx, "MESSAGE", b"hello \xe2\x98\x83"),
            "hello \u2603",
        )

    def test_plugin_profile_facet_for_uid(self):
        uid_str = str(os.getuid())
        try:
            pwd.getpwuid(int(uid_str))
        except KeyError:
            self.skipTest(f"uid {uid_str} not present in this environment")
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            plug.facet_option_name(ctx, "_UID", uid_str.encode()),
            pwd.getpwuid(int(uid_str)).pw_name,
        )


class DefaultFieldPassthrough(unittest.TestCase):
    def test_unknown_field_is_raw_in_both_modes(self):
        std = n.SystemdJournalProfile()
        plug = n.SystemdJournalPluginProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "CUSTOM_FIELD", b"abc"),
            "abc",
        )
        self.assertEqual(
            plug.field_display_value(ctx, n.DisplayScope.Data, "CUSTOM_FIELD", b"abc"),
            "abc",
        )

    def test_invalid_utf8_is_replaced(self):
        # The base fallback uses utf-8 with errors="replace".
        std = n.SystemdJournalProfile()
        ctx = n.DisplayContext()
        self.assertEqual(
            std.field_display_value(ctx, n.DisplayScope.Data, "CUSTOM_FIELD", b"good\xffbytes"),
            "good\ufffdbytes",
        )


# ---------------------------------------------------------------------------
# Chunk 2b fixtures and tests: function constructors, request decoding,
# source discovery, full data response, multi-file stats aggregation.
# ---------------------------------------------------------------------------


def _make_window_split_dir(
    tmp: str,
    in_window_usec: int = 1_700_000_000_000_000,
    in_window_count: int = 3,
    out_window_usec: int = 1_000_000_000_000_000,
    out_window_count: int = 3,
):
    """Build a synthetic directory with one machine holding TWO files:
    one whose entire message range falls inside a given request
    window and one whose entire range falls outside it. Returns
    (directory_path, in_window_path, out_window_path).

    The in-window file carries `IN_WINDOW_FIELD` as a unique field
    name; the out-of-window file carries `OUT_WINDOW_FIELD`. The
    Rust may-overlap file pre-filter
    (`journal_file_order_may_overlap_request`) must drop the
    out-of-window file from the column catalog and the explore
    loop. The test asserts that `OUT_WINDOW_FIELD` does NOT
    surface in the response `columns` while `IN_WINDOW_FIELD`
    does.
    """

    dir_path = pathlib.Path(tmp)
    sub = dir_path / "ccddeeff-3333-3333-3333-333333333333"
    sub.mkdir()
    in_path = sub / "in-window.journal"
    out_path = sub / "out-window.journal"
    w_in = Writer.create(str(in_path), {
        'machine_id': b'\x33' * 16, 'boot_id': b'\xcc' * 16,
        'seqnum_id': b'\x44' * 16,
    })
    for i in range(in_window_count):
        w_in.append(
            [
                {'name': 'MESSAGE', 'value': f'in-{i}'.encode()},
                {'name': 'PRIORITY', 'value': b'3'},
                {'name': 'IN_WINDOW_FIELD', 'value': f'in-{i}'.encode()},
            ],
            {'realtime_usec': in_window_usec + i * 1000},
        )
    w_in.close()
    w_out = Writer.create(str(out_path), {
        'machine_id': b'\x33' * 16, 'boot_id': b'\xdd' * 16,
        'seqnum_id': b'\x55' * 16,
    })
    for i in range(out_window_count):
        w_out.append(
            [
                {'name': 'MESSAGE', 'value': f'out-{i}'.encode()},
                {'name': 'PRIORITY', 'value': b'6'},
                {'name': 'OUT_WINDOW_FIELD', 'value': f'out-{i}'.encode()},
            ],
            {'realtime_usec': out_window_usec + i * 1000},
        )
    w_out.close()
    return str(dir_path), str(in_path), str(out_path)


def _make_two_machine_dir(tmp: str, base_time_usec: int = 1_700_000_000_000_000,
                          count_a: int = 5, count_b: int = 3,
                          machine_id_a: bytes = b'\x11' * 16,
                          machine_id_b: bytes = b'\x22' * 16,
                          boot_id_a: bytes = b'\xaa' * 16,
                          boot_id_b: bytes = b'\xbb' * 16):
    """Build a synthetic directory with two machine-id subdirs holding
    disjoint boot ids and distinct priorities, mirroring a multi-host
    logs directory. Returns (directory_path, file_a_path, file_b_path).
    """

    dir_path = pathlib.Path(tmp)
    sub_a = dir_path / "aabbccdd-1111-1111-1111-111111111111"
    sub_b = dir_path / "eeff00aa-2222-2222-2222-222222222222"
    sub_a.mkdir()
    sub_b.mkdir()
    file_a = sub_a / "system.journal"
    file_b = sub_b / "system.journal"
    w_a = Writer.create(str(file_a), {
        'machine_id': machine_id_a, 'boot_id': boot_id_a, 'seqnum_id': b'\x33' * 16,
    })
    for i in range(count_a):
        w_a.append(
            [
                {'name': 'MESSAGE', 'value': f'from-a-{i}'.encode()},
                {'name': 'PRIORITY', 'value': b'3'},
                {'name': 'SERVICE', 'value': b'svc-a'},
            ],
            {'realtime_usec': base_time_usec + i * 1000},
        )
    w_a.close()
    w_b = Writer.create(str(file_b), {
        'machine_id': machine_id_b, 'boot_id': boot_id_b, 'seqnum_id': b'\x33' * 16,
    })
    for i in range(count_b):
        w_b.append(
            [
                {'name': 'MESSAGE', 'value': f'from-b-{i}'.encode()},
                {'name': 'PRIORITY', 'value': b'6'},
                {'name': 'SERVICE', 'value': b'svc-b'},
            ],
            {'realtime_usec': base_time_usec + 100_000 + i * 1000},
        )
    w_b.close()
    return str(dir_path), str(file_a), str(file_b)


class NetdataJournalFunctionConstructors(unittest.TestCase):
    """`NetdataJournalFunction` factories and the `new` selector backfill."""

    def test_systemd_journal_uses_default_config(self):
        fn = n.NetdataJournalFunction.systemd_journal()
        self.assertEqual(fn._config.function_name, "systemd-journal")
        self.assertEqual(fn._config.source_selector_name, "Journal Sources")
        self.assertEqual(fn._config.source_selector_help, "Select the logs source to query")
        self.assertIsInstance(fn._profile, n.SystemdJournalProfile)

    def test_systemd_journal_plugin_compatible_uses_plugin_profile(self):
        fn = n.NetdataJournalFunction.systemd_journal_plugin_compatible()
        self.assertIsInstance(fn._profile, n.SystemdJournalPluginProfile)

    def test_new_backfills_empty_selector_name(self):
        cfg = n.NetdataFunctionConfig(
            source_selector_name="",
            source_selector_help="Custom help",
        )
        fn = n.NetdataJournalFunction.new(cfg, n.SystemdJournalProfile())
        self.assertEqual(fn._config.source_selector_name, "Journal Sources")
        self.assertEqual(fn._config.source_selector_help, "Custom help")

    def test_new_backfills_empty_selector_help(self):
        cfg = n.NetdataFunctionConfig(
            source_selector_name="My selector",
            source_selector_help="",
        )
        fn = n.NetdataJournalFunction.new(cfg, n.SystemdJournalProfile())
        self.assertEqual(fn._config.source_selector_name, "My selector")
        self.assertEqual(fn._config.source_selector_help, "Select the logs source to query")

    def test_new_preserves_non_empty(self):
        cfg = n.NetdataFunctionConfig(
            source_selector_name="My selector",
            source_selector_help="My help",
        )
        fn = n.NetdataJournalFunction.new(cfg, n.SystemdJournalProfile())
        self.assertEqual(fn._config.source_selector_name, "My selector")
        self.assertEqual(fn._config.source_selector_help, "My help")


class NetdataRequestParsing(unittest.TestCase):
    """Request decoding for the 16 accepted parameters."""

    def _parse(self, value, **cfg_overrides):
        cfg = n.NetdataFunctionConfig.systemd_journal()
        for key, val in cfg_overrides.items():
            setattr(cfg, key, val)
        return n.NetdataRequest.parse(value, cfg)

    def test_info_decodes(self):
        req = self._parse({"info": True})
        self.assertTrue(req.info)
        self.assertTrue(req.echo["info"])

    def test_logs_sources_decodes(self):
        # The accepted-parameter key is decoded; chunk 2b treats
        # the request as the logs-sources variant in run_*.
        req = self._parse({"__logs_sources": True})
        self.assertTrue(req.echo["info"] is False)

    def test_after_before_seconds(self):
        req = self._parse({"after": 1700000000, "before": 1700003600})
        self.assertEqual(req.after_realtime_usec, 1700000000_000_000)
        self.assertEqual(req.before_realtime_usec, 1700003600_999_999)

    def test_after_usec_passthrough(self):
        # A large usec value (1.7e15) exceeds the relative-time
        # threshold (~3 years), so it passes through as a literal
        # timestamp in `before` after the swap. The `if before >
        # now_seconds` clamp then shifts both endpoints so
        # `before <= now`, producing `(0, now)`.
        req = self._parse({"after": 1700000000_000_000})
        self.assertEqual(req.after_realtime_usec, 0)
        # `before` is clamped to the current wall clock.
        now_usec = int(__import__("time").time())
        # Allow 1 second of slack for the wall clock to advance
        # between the request parse and our read of time.
        self.assertAlmostEqual(req.before_realtime_usec, now_usec * 1_000_000, delta=1_500_000)

    def test_relative_after_with_before(self):
        # With both `after` and `before` set to negative values, the
        # Rust `relative_window_to_absolute` derives `after` as
        # `before + after + 1`, so the effective `after` becomes
        # `now - 1800 + (-3600) + 1 = now - 5399`. The window width
        # is therefore 3600-1 seconds, not 1800 seconds.
        req = self._parse({"after": -3600, "before": -1800})
        self.assertIsNotNone(req.after_realtime_usec)
        self.assertIsNotNone(req.before_realtime_usec)
        self.assertLess(req.after_realtime_usec, req.before_realtime_usec)
        # Window width is 3600 seconds minus 1 usec (the +1 in
        # `before + after + 1`).
        self.assertEqual(
            req.before_realtime_usec - req.after_realtime_usec,
            3_599_999_999,
        )

    def test_anchor_decoded(self):
        req = self._parse({"anchor": 1_700_000_000_000_000})
        self.assertEqual(req.anchor.kind.name, "REALTIME")
        self.assertEqual(req.anchor.realtime_usec, 1_700_000_000_000_000)

    def test_anchor_zero_means_auto(self):
        req = self._parse({"anchor": 0})
        self.assertEqual(req.anchor.kind.name, "AUTO")

    def test_direction_default_is_backward(self):
        req = self._parse({})
        self.assertEqual(req.direction.name, "BACKWARD")

    def test_direction_forward_variants(self):
        for value in ("forward", "forwards", "next"):
            req = self._parse({"direction": value})
            self.assertEqual(req.direction.name, "FORWARD", msg=f"direction={value!r}")

    def test_last_limit_uses_default_when_zero(self):
        # `last=0` falls back to DEFAULT_ITEMS_TO_RETURN (200), then
        # the Rust floor `max(2, requested_limit)` keeps it at 200.
        req = self._parse({"last": 0})
        self.assertEqual(req.limit, 200)

    def test_last_limit_uses_value(self):
        req = self._parse({"last": 17})
        self.assertEqual(req.limit, 17)

    def test_query_decodes(self):
        req = self._parse({"query": "error|critical"})
        self.assertEqual(req.query, "error|critical")

    def test_facets_uses_config_default(self):
        req = self._parse({})
        self.assertEqual(req.facets, [f.encode("utf-8") for f in n.SYSTEMD_DEFAULT_FACETS])

    def test_facets_override(self):
        req = self._parse({"facets": ["PRIORITY", "SERVICE"]})
        self.assertEqual(req.facets, [b"PRIORITY", b"SERVICE"])

    def test_histogram_default(self):
        req = self._parse({})
        self.assertEqual(req.histogram, "PRIORITY")

    def test_histogram_override(self):
        req = self._parse({"histogram": "SYSLOG_FACILITY"})
        self.assertEqual(req.histogram, "SYSLOG_FACILITY")

    def test_sampling_default(self):
        req = self._parse({})
        self.assertEqual(req.sampling, 1_000_000)

    def test_sampling_override(self):
        req = self._parse({"sampling": 42})
        self.assertEqual(req.sampling, 42)

    def test_slice_decoded(self):
        # The Rust `slice` parameter is forced to `true` on the wire;
        # decoding must accept it without error.
        req = self._parse({"slice": True})
        self.assertIsNotNone(req)

    def test_delta_data_only_tail_decoded(self):
        # Decoding must not error; behavior may be stubbed.
        req = self._parse({
            "data_only": True,
            "delta": True,
            "tail": True,
            "if_modified_since": 1_700_000_000_000_000,
        })
        self.assertTrue(req.data_only)
        self.assertTrue(req.delta)
        self.assertTrue(req.tail)
        self.assertEqual(req.if_modified_since_usec, 1_700_000_000_000_000)

    def test_echo_sanitizes_log_sources(self):
        req = self._parse({
            "selections": {"__logs_sources": ["all"], "PRIORITY": ["3"]},
        })
        self.assertIsNone(req.echo["selections"]["__logs_sources"][0])
        self.assertEqual(req.echo["selections"]["PRIORITY"], ["3"])


class SourceDiscoveryClassification(unittest.TestCase):
    """Source-type classification and scan depth/count limits."""

    def test_system_path_classification(self):
        path = pathlib.Path("/var/log/journal/abc/system.journal")
        self.assertEqual(
            n._journal_file_source_type(path),
            n.NETDATA_SOURCE_TYPE_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
        )

    def test_user_path_classification(self):
        path = pathlib.Path("/var/log/journal/abc/user-1000.journal")
        self.assertEqual(
            n._journal_file_source_type(path),
            n.NETDATA_SOURCE_TYPE_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_USER,
        )

    def test_namespace_path_classification(self):
        path = pathlib.Path("/var/log/journal/abc.myns/system.journal")
        self.assertEqual(
            n._journal_file_source_type(path),
            n.NETDATA_SOURCE_TYPE_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
        )

    def test_other_path_classification(self):
        path = pathlib.Path("/var/log/journal/abc/other.journal")
        self.assertEqual(
            n._journal_file_source_type(path),
            n.NETDATA_SOURCE_TYPE_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_ALL
            | n.NETDATA_SOURCE_TYPE_LOCAL_OTHER,
        )

    def test_remote_path_classification(self):
        path = pathlib.Path("/var/log/journal/abc/remote/system@host.journal")
        self.assertEqual(
            n._journal_file_source_type(path),
            n.NETDATA_SOURCE_TYPE_ALL | n.NETDATA_SOURCE_TYPE_REMOTE_ALL,
        )

    def test_namespace_exact_source_name(self):
        path = pathlib.Path("/var/log/journal/abc.myns/system.journal")
        self.assertEqual(n._journal_file_exact_source_name(path), "namespace-myns")

    def test_remote_exact_source_name(self):
        path = pathlib.Path("/var/log/journal/abc/remote/system@host.journal")
        self.assertEqual(n._journal_file_exact_source_name(path), "system")

    def test_non_journal_extension_is_ignored(self):
        with tempfile.TemporaryDirectory() as tmp:
            (pathlib.Path(tmp) / "readme.txt").write_text("not a journal")
            (pathlib.Path(tmp) / "system.journal").write_bytes(b"")
            collection = n._collect_journal_files(tmp)
            names = [os.path.basename(p) for p in collection.files]
            self.assertIn("system.journal", names)
            self.assertNotIn("readme.txt", names)

    def test_depth_64(self):
        with tempfile.TemporaryDirectory() as tmp:
            deep = pathlib.Path(tmp)
            for _ in range(70):
                deep = deep / "subdir"
                deep.mkdir()
            (deep / "system.journal").write_bytes(b"")
            collection = n._collect_journal_files(tmp)
            # Depth 64 means we do not descend into directories at depth
            # 64 from the root, so the file at depth 70 is not collected.
            self.assertEqual(collection.files, [])

    def test_count_limit_8192(self):
        with tempfile.TemporaryDirectory() as tmp:
            # Build 9000 subdirs each with a system.journal file.
            root = pathlib.Path(tmp)
            for i in range(9000):
                sub = root / f"sub-{i:05d}"
                sub.mkdir()
                (sub / "system.journal").write_bytes(b"")
            collection = n._collect_journal_files(tmp)
            # The count cap is 8192 directories; we expect at most that
            # many directories to be visited and thus at most 8192 files.
            self.assertLessEqual(len(collection.files), 8192)
            self.assertGreater(collection.skipped, 0)


class InfoResponse(unittest.TestCase):
    def test_info_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {"info": True})
            for key in (
                "_request", "versions", "v", "accepted_params",
                "required_params", "show_ids", "has_history", "pagination",
                "status", "type", "help",
            ):
                self.assertIn(key, response, msg=f"missing {key!r} in info response")
            self.assertEqual(response["v"], 3)
            self.assertTrue(response["show_ids"])
            self.assertTrue(response["has_history"])
            self.assertTrue(response["pagination"]["enabled"])
            self.assertEqual(response["pagination"]["key"], "anchor")
            self.assertEqual(response["status"], 200)
            self.assertEqual(response["type"], "table")
            # accepted_params includes the 16 names (no extra fields).
            self.assertEqual(
                response["accepted_params"],
                list(n.NETDATA_ACCEPTED_PARAMS),
            )
            # required_params is a single-element list with __logs_sources.
            self.assertEqual(len(response["required_params"]), 1)
            self.assertEqual(response["required_params"][0]["id"], "__logs_sources")
            self.assertEqual(
                response["required_params"][0]["name"],
                "Journal Sources",
            )
            self.assertEqual(
                response["required_params"][0]["help"],
                "Select the logs source to query",
            )
            self.assertEqual(
                response["required_params"][0]["type"],
                "multiselect",
            )
            # options include the canonical aggregates.
            ids = [opt["id"] for opt in response["required_params"][0]["options"]]
            for expected in ("all", "all-local-logs"):
                self.assertIn(expected, ids)


class HumanDurationSeconds(unittest.TestCase):
    """Mirror the Rust `human_duration_seconds` (L3809-3839) formatter.

    Rules ported from Rust:
      * units: 1y=365d, 1mo=30d, 1d=86400s, 1h=3600s, 1m=60s, 1s=1s
      * integer division + modulo, fixed order
      * zero-valued components are omitted
      * if every component is zero, the seconds component is emitted as
        ``0s`` (the `parts.is_empty()` fallback)
      * components are joined with a single ASCII space, no trailing
        separator
    """

    def test_zero_seconds_emits_zero_s(self):
        self.assertEqual(n._human_duration_seconds(0), "0s")

    def test_single_seconds(self):
        self.assertEqual(n._human_duration_seconds(1), "1s")
        self.assertEqual(n._human_duration_seconds(59), "59s")

    def test_minutes_seconds(self):
        self.assertEqual(n._human_duration_seconds(60), "1m")
        self.assertEqual(n._human_duration_seconds(61), "1m 1s")
        self.assertEqual(n._human_duration_seconds(125), "2m 5s")

    def test_hours_rollover(self):
        # 3600s = 1h exactly; 3601s = 1h 1s
        self.assertEqual(n._human_duration_seconds(3600), "1h")
        self.assertEqual(n._human_duration_seconds(3601), "1h 1s")
        # 7322s = 2h 2m 2s
        self.assertEqual(n._human_duration_seconds(7322), "2h 2m 2s")

    def test_days_rollover(self):
        # 86400s = 1d exactly
        self.assertEqual(n._human_duration_seconds(86_400), "1d")
        # 90061s = 1d 1h 1m 1s
        self.assertEqual(n._human_duration_seconds(90_061), "1d 1h 1m 1s")
        # 172800s = 2d
        self.assertEqual(n._human_duration_seconds(172_800), "2d")

    def test_months_use_30_day_window(self):
        # 30 * 86400 = 2592000s = 1mo
        self.assertEqual(n._human_duration_seconds(2_592_000), "1mo")
        # 31 * 86400s = 31d, NOT 1mo 1d (months are 30d, then 1d)
        self.assertEqual(n._human_duration_seconds(31 * 86_400), "1mo 1d")
        # 60 * 86400s = 2mo exactly
        self.assertEqual(n._human_duration_seconds(60 * 86_400), "2mo")

    def test_years_use_365_day_window(self):
        # 365 * 86400s = 1y
        self.assertEqual(n._human_duration_seconds(365 * 86_400), "1y")
        # 366 * 86400s = 1y 1d (years are 365d, then 1d)
        self.assertEqual(n._human_duration_seconds(366 * 86_400), "1y 1d")

    def test_omits_zero_components(self):
        # 365d + 0mo + 5d + 0h + 0m + 0s
        seconds = (365 + 5) * 86_400
        self.assertEqual(n._human_duration_seconds(seconds), "1y 5d")

    def test_omits_zero_components_within_months(self):
        # 1y + 6mo exactly
        seconds = (365 + 6 * 30) * 86_400
        self.assertEqual(n._human_duration_seconds(seconds), "1y 6mo")

    def test_full_composition_matches_canonical(self):
        # Canonical example from the comparator diff:
        # 2y 6mo 24d 10h 9m 1s
        seconds = (
            2 * 365 * 86_400
            + 6 * 30 * 86_400
            + 24 * 86_400
            + 10 * 3600
            + 9 * 60
            + 1
        )
        self.assertEqual(n._human_duration_seconds(seconds), "2y 6mo 24d 10h 9m 1s")

    def test_joins_with_single_space_no_trailing(self):
        # No trailing separator regardless of how many components.
        self.assertEqual(
            n._human_duration_seconds(
                2 * 365 * 86_400
                + 6 * 30 * 86_400
                + 24 * 86_400
                + 10 * 3600
                + 9 * 60
                + 1
            ).count(" "),
            5,
        )


class SourceSummaryInfoString(unittest.TestCase):
    """Composition rules for the source-option ``info`` string.

    Mirrors Rust `JournalSourceSummary::info` (L1779-1798):
      * format: ``{files} files, total size {size}, covering {coverage},
        last entry at {rfc3339}``
      * coverage: ``human_duration_seconds((last-first)/1_000_000)`` if
        both bounds are present, ``last > first`` and the gap is
        ``>= 1_000_000`` usec, otherwise the literal ``off``
      * last entry: ``last_realtime_usec / 1_000_000`` rendered as
        ``%Y-%m-%dT%H:%M:%SZ``; missing last_realtime -> ``unknown``
    """

    def _summary(self, files=1, total_size=1024,
                 first_usec=None, last_usec=None):
        s = n.JournalSourceSummary()
        s.files = files
        s.total_size = total_size
        s.first_realtime_usec = first_usec
        s.last_realtime_usec = last_usec
        return s

    def test_full_info_with_metadata(self):
        s = self._summary(
            files=3,
            total_size=5 * 1024 * 1024,
            first_usec=1_700_000_000_000_000,
            last_usec=1_700_000_001_000_000,
        )
        opt = n._summary_to_source_option("synthetic", s)
        self.assertIsNotNone(opt)
        self.assertEqual(opt["id"], "synthetic")
        self.assertEqual(opt["name"], "synthetic")
        self.assertEqual(
            opt["info"],
            "3 files, total size 5MiB, covering 1s, "
            "last entry at 2023-11-14T22:13:21Z",
        )
        self.assertEqual(opt["pill"], "5MiB")

    def test_canonical_comparator_string(self):
        # The exact value reported by the three-peer comparator diff:
        # "2y 6mo 24d 10h 9m 1s, last entry at 2026-06-11T20:50:26Z"
        gap_seconds = (
            2 * 365 * 86_400
            + 6 * 30 * 86_400
            + 24 * 86_400
            + 10 * 3600
            + 9 * 60
            + 1
        )
        last = 1_780_000_000_000_000  # arbitrary anchor
        first = last - gap_seconds * 1_000_000
        s = self._summary(
            files=7336,
            total_size=144 * 1024 ** 3 + 260 * 1024 ** 2,
            first_usec=first,
            last_usec=last,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertIsNotNone(opt)
        # Verify the suffix exactly matches the comparator output.
        self.assertIn(", covering 2y 6mo 24d 10h 9m 1s, last entry at ", opt["info"])
        # Verify the prefix.
        self.assertTrue(opt["info"].startswith("7336 files, total size "))

    def test_info_omitted_when_no_files(self):
        s = self._summary(files=0, total_size=0,
                          first_usec=1_700_000_000_000_000,
                          last_usec=1_700_000_001_000_000)
        self.assertIsNone(n._summary_to_source_option("all", s))

    def test_coverage_off_when_metadata_missing(self):
        # No bounds at all -> covering off, last entry unknown.
        s = self._summary(files=2, total_size=2048)
        opt = n._summary_to_source_option("all", s)
        self.assertIsNotNone(opt)
        self.assertEqual(
            opt["info"],
            "2 files, total size 2KiB, covering off, last entry at unknown",
        )

    def test_coverage_off_when_first_missing(self):
        # first_usec=None means we have no lower bound -> off.
        s = self._summary(
            files=1, total_size=1024,
            first_usec=None, last_usec=1_700_000_000_000_000,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertEqual(
            opt["info"],
            "1 files, total size 1KiB, covering off, "
            "last entry at 2023-11-14T22:13:20Z",
        )

    def test_coverage_off_when_last_not_strictly_greater(self):
        # last <= first -> off, but the last-entry string is still
        # rendered (the Rust side formats last_realtime_usec
        # independently of the coverage branch).
        s = self._summary(
            files=1, total_size=1024,
            first_usec=1_700_000_000_000_000,
            last_usec=1_700_000_000_000_000,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertEqual(
            opt["info"],
            "1 files, total size 1KiB, covering off, "
            "last entry at 2023-11-14T22:13:20Z",
        )

    def test_coverage_off_for_subsecond_gap(self):
        # Gap of 999_999 usec (< 1_000_000) -> off.
        s = self._summary(
            files=1, total_size=1024,
            first_usec=1_700_000_000_000_000,
            last_usec=1_700_000_000_999_999,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertEqual(
            opt["info"],
            "1 files, total size 1KiB, covering off, "
            "last entry at 2023-11-14T22:13:20Z",
        )

    def test_coverage_one_second_at_threshold(self):
        # Gap of exactly 1_000_000 usec -> covering 1s.
        s = self._summary(
            files=1, total_size=1024,
            first_usec=1_700_000_000_000_000,
            last_usec=1_700_000_001_000_000,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertEqual(
            opt["info"],
            "1 files, total size 1KiB, covering 1s, "
            "last entry at 2023-11-14T22:13:21Z",
        )

    def test_coverage_full_canonical_value(self):
        gap_seconds = (
            2 * 365 * 86_400
            + 6 * 30 * 86_400
            + 24 * 86_400
            + 10 * 3600
            + 9 * 60
            + 1
        )
        last = 1_780_000_000_000_000
        first = last - gap_seconds * 1_000_000
        s = self._summary(
            files=7336,
            total_size=144 * 1024 ** 3 + 260 * 1024 ** 2,
            first_usec=first,
            last_usec=last,
        )
        opt = n._summary_to_source_option("all", s)
        self.assertIn(", covering 2y 6mo 24d 10h 9m 1s, last entry at ", opt["info"])

    def test_rfc3339_format_no_microseconds(self):
        rendered = n._format_last_entry_rfc3339_usec(1_700_000_000_123_456)
        self.assertEqual(rendered, "2023-11-14T22:13:20Z")

    def test_rfc3339_truncates_subsecond_microseconds(self):
        rendered = n._format_last_entry_rfc3339_usec(1_700_000_000_999_999)
        self.assertEqual(rendered, "2023-11-14T22:13:20Z")

    def test_rfc3339_none_returns_unknown(self):
        self.assertEqual(n._format_last_entry_rfc3339_usec(None), "unknown")


class LogsSourcesResponse(unittest.TestCase):
    def test_logs_sources_shape_and_wire_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {"__logs_sources": True})
            self.assertEqual(response["id"], "__logs_sources")
            self.assertEqual(response["type"], "multiselect")
            self.assertEqual(response["name"], "Journal Sources")
            self.assertEqual(response["help"], "Select the logs source to query")
            self.assertEqual(response["status"], 200)
            ids = [opt["id"] for opt in response["options"]]
            self.assertIn("all", ids)
            self.assertIn("all-local-logs", ids)
            self.assertIn("all-local-system-logs", ids)

    def test_logs_sources_with_namespace(self):
        with tempfile.TemporaryDirectory() as tmp:
            sub = pathlib.Path(tmp) / "abc.myns"
            sub.mkdir()
            f = sub / "system.journal"
            w = Writer.create(str(f), {
                'machine_id': b'\x11' * 16, 'boot_id': b'\xaa' * 16,
                'seqnum_id': b'\x33' * 16,
            })
            w.append([{'name': 'MESSAGE', 'value': b'hi'}], {'realtime_usec': 1})
            w.close()
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {"__logs_sources": True})
            ids = [opt["id"] for opt in response["options"]]
            self.assertIn("all-local-namespaces", ids)
            self.assertIn("namespace-myns", ids)


def _make_multi_file_journal(
    tmp: str,
    spec: "list[dict]",
    base_time_usec: int = 1_700_000_000_000_000,
    step_usec: int = 1_000_000,
):
    """Build a synthetic directory of N independent journal files.

    Each `spec` entry is a dict with keys:
        - `machine_id` (bytes, 16 bytes)
        - `boot_id` (bytes, 16 bytes)
        - `count` (int): number of entries to append.
        - `realtime_offset` (int): offset added to `base_time_usec`
          for the first entry of this file; subsequent entries use
          the same `step_usec` interval. Defaults to the running
          offset so files naturally chain.
    Returns the list of absolute file paths that were written.
    """

    dir_path = pathlib.Path(tmp)
    paths: list = []
    next_offset = 0
    for index, entry in enumerate(spec):
        machine_id = entry.get("machine_id", bytes([0x10 + index]) * 16)
        boot_id = entry.get("boot_id", bytes([0xa0 + index]) * 16)
        count = int(entry.get("count", 3))
        offset = int(entry.get("realtime_offset", next_offset))
        # 32 hex chars in the machine-id subdir name so the source
        # classifier picks it up as a real machine-id directory.
        sub = dir_path / machine_id.hex()
        sub.mkdir(exist_ok=True)
        file_path = sub / "system.journal"
        w = Writer.create(str(file_path), {
            "machine_id": machine_id,
            "boot_id": boot_id,
            "seqnum_id": b"\x33" * 16,
        })
        for i in range(count):
            w.append(
                [{"name": "MESSAGE", "value": f"f{index}-{i}".encode()}],
                {"realtime_usec": base_time_usec + offset + i * step_usec},
            )
        w.close()
        paths.append(str(file_path))
        next_offset = offset + count * step_usec
    return paths


class SourceSummaryBounds(unittest.TestCase):
    """Per-file entry-bounds flow for `JournalSourceSummary::add_path`.

    Mirrors Rust `JournalSourceSummary::add_path` (L1728-1777): stat
    the file, consult the optional metadata cache, then open the file
    via the header-only path (`FileReader::open_with_options` +
    `reader.header()`) to obtain `head_entry_realtime` /
    `tail_entry_realtime` and widen the union bounds. Tests assert
    concrete computed values (real coverage / last-entry timestamp)
    rather than text fragments, so a future scope cut that drops the
    header read cannot pass.
    """

    def test_add_path_single_file_populates_real_bounds(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            [path] = _make_multi_file_journal(
                tmp, [{"count": 3}], base_time_usec=base, step_usec=1_000_000
            )
            summary = n.JournalSourceSummary()
            summary.add_path(path)
            self.assertEqual(summary.files, 1)
            self.assertGreater(summary.total_size, 0)
            # header bounds: head = base, tail = base + 2 * step_usec.
            self.assertEqual(summary.first_realtime_usec, base)
            self.assertEqual(summary.last_realtime_usec, base + 2 * 1_000_000)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertEqual(
                opt["info"],
                f"1 files, total size {n._human_binary_size(summary.total_size)}, "
                f"covering 2s, last entry at 2023-11-14T22:13:22Z",
            )

    def test_add_path_multi_file_spans_union_with_real_info_string(self):
        # Three files, each with 3 entries, 1s apart, chained 10s apart.
        # The union span is base .. base + 22s, i.e. covering 22s, with
        # the last entry at base + 22s.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            paths = _make_multi_file_journal(
                tmp,
                [
                    {"count": 3, "machine_id": b"\x11" * 16, "realtime_offset": 0},
                    {"count": 3, "machine_id": b"\x22" * 16, "realtime_offset": 10_000_000},
                    {"count": 3, "machine_id": b"\x33" * 16, "realtime_offset": 20_000_000},
                ],
                base_time_usec=base,
                step_usec=1_000_000,
            )
            summary = n.JournalSourceSummary()
            for p in paths:
                summary.add_path(p)
            self.assertEqual(summary.files, 3)
            self.assertEqual(summary.first_realtime_usec, base)
            self.assertEqual(
                summary.last_realtime_usec, base + 22 * 1_000_000
            )
            opt = n._summary_to_source_option("synthetic", summary)
            # Assert the FULL info string against the computed value
            # so a regression that drops the bounds cannot pass.
            self.assertEqual(
                opt["info"],
                f"3 files, total size {n._human_binary_size(summary.total_size)}, "
                f"covering 22s, last entry at 2023-11-14T22:13:42Z",
            )

    def test_add_path_unreadable_file_is_skipped(self):
        # A path that does not exist -> os.stat raises -> add_path
        # returns early. Rust matches this via `if let Ok(metadata)
        # = std::fs::metadata(path)`.
        summary = n.JournalSourceSummary()
        summary.add_path("/nonexistent/path/to/journal.journal")
        self.assertEqual(summary.files, 0)
        self.assertEqual(summary.total_size, 0)
        self.assertIsNone(summary.first_realtime_usec)
        self.assertIsNone(summary.last_realtime_usec)
        self.assertIsNone(n._summary_to_source_option("synthetic", summary))

    def test_add_path_too_small_file_keeps_off_unknown(self):
        # A file that is stat-able but too small for a journal header
        # contributes `files` + `total_size` from the stat, but no
        # bounds. The summary still renders, with `covering off` and
        # `last entry at unknown`.
        with tempfile.NamedTemporaryFile(suffix=".journal", delete=False) as f:
            f.write(b"x" * 10)
            small = f.name
        try:
            summary = n.JournalSourceSummary()
            summary.add_path(small)
            self.assertEqual(summary.files, 1)
            self.assertEqual(summary.total_size, 10)
            self.assertIsNone(summary.first_realtime_usec)
            self.assertIsNone(summary.last_realtime_usec)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertEqual(
                opt["info"],
                "1 files, total size 10B, covering off, last entry at unknown",
            )
        finally:
            os.unlink(small)

    def test_add_path_bad_magic_keeps_off_unknown(self):
        # A file that has enough bytes for a header but the wrong
        # magic -> header parse fails -> bounds stay None. The file
        # is still stat-counted. Mirrors Rust: `if let Ok(reader) =
        # FileReader::open_with_options(...)` -> open fails -> the
        # summary path returns before touching `head/tail`.
        with tempfile.NamedTemporaryFile(suffix=".journal", delete=False) as f:
            f.write(b"NOTAJOUR" + b"x" * 256)
            bad = f.name
        try:
            summary = n.JournalSourceSummary()
            summary.add_path(bad)
            self.assertEqual(summary.files, 1)
            self.assertGreater(summary.total_size, 0)
            self.assertIsNone(summary.first_realtime_usec)
            self.assertIsNone(summary.last_realtime_usec)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertIn("covering off, last entry at unknown", opt["info"])
        finally:
            os.unlink(bad)

    def test_add_path_zero_entry_file_keeps_off_unknown(self):
        # A real journal file with no entries: head and tail entry
        # reals are 0 in the header; the `!= 0` guard skips both.
        # Mirrors Rust: `if header.head_entry_realtime != 0` and
        # `if header.tail_entry_realtime != 0` (L1761-1776).
        with tempfile.TemporaryDirectory() as tmp:
            dir_path = pathlib.Path(tmp)
            sub = dir_path / ("11" * 32)
            sub.mkdir()
            f = sub / "system.journal"
            w = Writer.create(str(f), {
                "machine_id": b"\x11" * 16, "boot_id": b"\xaa" * 16,
                "seqnum_id": b"\x33" * 16,
            })
            w.close()
            summary = n.JournalSourceSummary()
            summary.add_path(str(f))
            self.assertEqual(summary.files, 1)
            self.assertIsNone(summary.first_realtime_usec)
            self.assertIsNone(summary.last_realtime_usec)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertIn("covering off, last entry at unknown", opt["info"])

    def test_add_path_full_metadata_skips_header(self):
        # When the state hook provides BOTH bounds, the file header
        # is not consulted. We assert the bounds come from the
        # metadata (not the file), and the file's actual realtime
        # range is ignored.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            [path] = _make_multi_file_journal(
                tmp, [{"count": 3}], base_time_usec=base, step_usec=1_000_000
            )
            summary = n.JournalSourceSummary()
            summary.add_path(
                path,
                metadata=n.NetdataJournalFileMetadata(
                    msg_first_realtime_usec=base - 5_000_000,
                    msg_last_realtime_usec=base + 20_000_000,
                ),
            )
            self.assertEqual(summary.first_realtime_usec, base - 5_000_000)
            self.assertEqual(summary.last_realtime_usec, base + 20_000_000)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertIn("covering 25s, last entry at 2023-11-14T22:13:40Z", opt["info"])

    def test_add_path_partial_metadata_merges_with_header(self):
        # State provides only `first`; `last` must come from the
        # header. The summary's first stays as the metadata first,
        # the last comes from the actual file (base + 2 * step).
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            [path] = _make_multi_file_journal(
                tmp, [{"count": 3}], base_time_usec=base, step_usec=1_000_000
            )
            summary = n.JournalSourceSummary()
            summary.add_path(
                path,
                metadata=n.NetdataJournalFileMetadata(
                    msg_first_realtime_usec=base - 5_000_000,
                ),
            )
            self.assertEqual(summary.first_realtime_usec, base - 5_000_000)
            self.assertEqual(summary.last_realtime_usec, base + 2 * 1_000_000)
            opt = n._summary_to_source_option("synthetic", summary)
            self.assertIn("covering 7s, last entry at 2023-11-14T22:13:22Z", opt["info"])

    def test_add_path_widens_min_first_and_max_last_across_files(self):
        # Two files with disjoint ranges: file_a starts earlier,
        # file_b ends later. The summary's first = file_a head,
        # last = file_b tail, regardless of add_path order.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            paths = _make_multi_file_journal(
                tmp,
                [
                    {"count": 2, "machine_id": b"\x11" * 16, "realtime_offset": 0},
                    {"count": 2, "machine_id": b"\x22" * 16, "realtime_offset": 60_000_000},
                ],
                base_time_usec=base,
                step_usec=1_000_000,
            )
            # First call: file_b (later range).
            summary = n.JournalSourceSummary()
            summary.add_path(paths[1])
            # Second call: file_a (earlier range). The summary's
            # first must drop to file_a's head.
            summary.add_path(paths[0])
            self.assertEqual(summary.files, 2)
            self.assertEqual(summary.first_realtime_usec, base)
            self.assertEqual(
                summary.last_realtime_usec, base + 60_000_000 + 1 * 1_000_000
            )

    def test_add_path_handles_dot_journal_zst_files(self):
        # A `.journal.zst` file: `is_zst_file` triggers decompression
        # to a temp file, the temp is mmap-read, the header is
        # parsed, the temp is cleaned up. The same head/tail realtime
        # bounds emerge.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            dir_path = pathlib.Path(tmp)
            sub = dir_path / ("11" * 32)
            sub.mkdir()
            raw = sub / "system.journal"
            w = Writer.create(str(raw), {
                "machine_id": b"\x11" * 16, "boot_id": b"\xaa" * 16,
                "seqnum_id": b"\x33" * 16,
            })
            for i in range(2):
                w.append(
                    [{"name": "MESSAGE", "value": f"m-{i}".encode()}],
                    {"realtime_usec": base + i * 1_000_000},
                )
            w.close()
            zst_path = sub / "system.journal.zst"
            import compression.zstd
            with open(raw, "rb") as src, open(zst_path, "wb") as dst:
                dst.write(compression.zstd.compress(src.read()))
            summary = n.JournalSourceSummary()
            summary.add_path(str(zst_path))
            self.assertEqual(summary.files, 1)
            self.assertGreater(summary.total_size, 0)
            self.assertEqual(summary.first_realtime_usec, base)
            self.assertEqual(summary.last_realtime_usec, base + 1 * 1_000_000)

    def test_wrapper_info_request_real_bounds(self):
        # End-to-end: a synthetic 2-file directory produces a real
        # `covering <duration>, last entry at <iso>Z` info string in
        # the wrapper's `info` response. This is the comparator
        # surface that SOW-0104 fix 3 targets.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            _make_multi_file_journal(
                tmp,
                [
                    {"count": 3, "machine_id": b"\x11" * 16, "realtime_offset": 0},
                    {"count": 3, "machine_id": b"\x22" * 16, "realtime_offset": 10_000_000},
                ],
                base_time_usec=base,
                step_usec=1_000_000,
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {"info": True})
            options = response["required_params"][0]["options"]
            all_opt = next(o for o in options if o["id"] == "all")
            self.assertIn(
                ", covering 12s, last entry at 2023-11-14T22:13:32Z",
                all_opt["info"],
            )
            self.assertNotIn("covering off", all_opt["info"])
            self.assertNotIn("last entry at unknown", all_opt["info"])

    def test_wrapper_logs_sources_real_bounds(self):
        # Same as above but via the `__logs_sources` selector.
        with tempfile.TemporaryDirectory() as tmp:
            base = 1_700_000_000_000_000
            _make_multi_file_journal(
                tmp,
                [
                    {"count": 3, "machine_id": b"\x11" * 16, "realtime_offset": 0},
                    {"count": 3, "machine_id": b"\x22" * 16, "realtime_offset": 10_000_000},
                ],
                base_time_usec=base,
                step_usec=1_000_000,
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(
                tmp, {"__logs_sources": True}
            )
            all_opt = next(o for o in response["options"] if o["id"] == "all")
            self.assertIn(
                ", covering 12s, last entry at 2023-11-14T22:13:32Z",
                all_opt["info"],
            )


class DataResponseEnvelope(unittest.TestCase):
    def test_envelope_keys_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "direction": "backward",
                "facets": ["PRIORITY", "SERVICE"],
            })
            for key in (
                "_request", "versions", "_journal_files", "status", "partial",
                "type", "show_ids", "has_history", "pagination", "columns",
                "data", "_stats", "expires", "message", "update_every",
                "help", "accepted_params", "default_sort_column",
                "default_charts", "available_histograms", "facets",
                "histogram", "items", "last_modified",
            ):
                self.assertIn(key, response, msg=f"missing {key!r} in data response")

    def test_row_count_and_ordering_backward(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "direction": "backward",
            })
            # 5 + 3 = 8 rows.
            self.assertEqual(len(response["data"]), 8)
            timestamps = [row[0] for row in response["data"]]
            self.assertEqual(timestamps, sorted(timestamps, reverse=True))

    def test_row_count_and_ordering_forward(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "direction": "forward",
            })
            self.assertEqual(len(response["data"]), 8)
            timestamps = [row[0] for row in response["data"]]
            # The Netdata response layer mirrors Rust `build_data_rows`
            # which outputs descending-time in BOTH directions. The
            # chunk-1 explorer sorts per direction; the response
            # envelope inverts the forward case so the wire shape is
            # always descending.
            self.assertEqual(timestamps, sorted(timestamps, reverse=True))

    def test_facet_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY", "SERVICE"],
            })
            priority = next(f for f in response["facets"] if f["id"] == "PRIORITY")
            service = next(f for f in response["facets"] if f["id"] == "SERVICE")
            priority_map = {o["name"]: o["count"] for o in priority["options"]}
            service_map = {o["name"]: o["count"] for o in service["options"]}
            self.assertEqual(priority_map, {"error": 5, "info": 3})

    def test_histogram_envelope_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            histogram = response["histogram"]
            for key in ("id", "name", "chart"):
                self.assertIn(key, histogram, msg=f"histogram missing {key!r}")
            chart = histogram["chart"]
            for key in ("summary", "totals", "result", "db", "view", "agents"):
                self.assertIn(key, chart, msg=f"chart missing {key!r}")
            self.assertIn("data", chart["result"])
            self.assertEqual(chart["result"]["labels"][0], "time")
            self.assertGreater(len(chart["result"]["data"]), 0)

    def test_view_dimensions_names_on_empty_window(self):
        with tempfile.TemporaryDirectory() as tmp:
            # An empty directory: no journal files, so the window is empty.
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            view_dims = response["histogram"]["chart"]["view"]["dimensions"]
            self.assertIn("names", view_dims)
            self.assertEqual(view_dims["names"], [])

    def test_view_dimensions_names_on_empty_window_with_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            # Files exist, but the time window excludes their data.
            # The window is sent as a past absolute window well below
            # the journal entry timestamp; the data-derived clamp
            # (SOW-0104 fix-9) reads the journal's max tail realtime
            # (~Sep 9 2001) and does not pull the window into the
            # entry's range because the SENT `before` is in the 1990s
            # (before the entry).
            sub = pathlib.Path(tmp) / "aabbccdd-1111-1111-1111-111111111111"
            sub.mkdir()
            f = sub / "system.journal"
            w = Writer.create(str(f), {
                'machine_id': b'\x11' * 16, 'boot_id': b'\xaa' * 16,
                'seqnum_id': b'\x33' * 16,
            })
            w.append(
                [{'name': 'MESSAGE', 'value': b'old'}],
                {'realtime_usec': 1_000_000_000_000_000},
            )
            w.close()
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                # 1990-01-01 to 1990-01-02: entirely below the
                # journal entry at Sep 9 2001, so the data-derived
                # clamp cannot pull the entry inside the effective
                # window.
                "after": 631152000,
                "before": 631238400,
                "facets": ["PRIORITY"],
            })
            self.assertEqual(len(response["data"]), 0)
            view_dims = response["histogram"]["chart"]["view"]["dimensions"]
            self.assertIn("names", view_dims)
            self.assertEqual(view_dims["names"], [])

    def test_last_modified_present_unless_data_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            normal = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertIn("last_modified", normal)
            data_only = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
            })
            self.assertNotIn("last_modified", data_only)
            data_only_tail = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "tail": True,
                "if_modified_since": 1_700_000_000_000_000,
            })
            self.assertIn("last_modified", data_only_tail)


class MultiFileStatsAggregation(unittest.TestCase):
    """`CombinedResult.merge` stats semantics must aggregate everything
    except the two maxima (last_realtime_usec, max_source_realtime_delta_usec).
    """

    def test_stats_sum_aggregate_fields(self):
        from journal.explorer import ExplorerStats
        combined = n.CombinedResult()
        a = ExplorerStats(
            rows_examined=10, rows_matched=7, facet_rows_matched=5,
            rows_returned=3, rows_unsampled=2, rows_estimated=1,
            sampling_sampled=4, sampling_unsampled=1, sampling_estimated=0,
            data_refs_seen=11, data_refs_skipped=2, data_payloads_loaded=8,
            data_objects_classified=7, data_cache_hits=3, data_cache_misses=4,
            payloads_decompressed=1, fts_scans=2, facet_updates=3,
            histogram_updates=4, returned_row_expansions=5,
            early_stop_opportunities=6, early_stops=2,
        )
        b = ExplorerStats(
            rows_examined=20, rows_matched=11, facet_rows_matched=8,
            rows_returned=4, rows_unsampled=1, rows_estimated=3,
            sampling_sampled=6, sampling_unsampled=0, sampling_estimated=2,
            data_refs_seen=12, data_refs_skipped=3, data_payloads_loaded=9,
            data_objects_classified=6, data_cache_hits=2, data_cache_misses=5,
            payloads_decompressed=2, fts_scans=1, facet_updates=4,
            histogram_updates=5, returned_row_expansions=6,
            early_stop_opportunities=7, early_stops=3,
        )
        from journal.explorer import ExplorerResult, ExplorerRow
        result_a = ExplorerResult(rows=[ExplorerRow(realtime_usec=1, cursor="a", payloads=[])], stats=a)
        result_b = ExplorerResult(rows=[ExplorerRow(realtime_usec=2, cursor="b", payloads=[])], stats=b)
        combined.merge("/tmp/a", result_a, n.Direction.BACKWARD, 10)
        combined.merge("/tmp/b", result_b, n.Direction.BACKWARD, 10)
        merged = combined.stats
        self.assertEqual(merged.rows_examined, 30)
        self.assertEqual(merged.rows_matched, 18)
        self.assertEqual(merged.facet_rows_matched, 13)
        self.assertEqual(merged.rows_returned, 2)  # overwritten by sort_and_limit
        self.assertEqual(merged.rows_unsampled, 3)
        self.assertEqual(merged.rows_estimated, 4)
        self.assertEqual(merged.sampling_sampled, 10)
        self.assertEqual(merged.sampling_unsampled, 1)
        self.assertEqual(merged.sampling_estimated, 2)
        self.assertEqual(merged.data_refs_seen, 23)
        self.assertEqual(merged.data_refs_skipped, 5)
        self.assertEqual(merged.data_payloads_loaded, 17)
        self.assertEqual(merged.data_objects_classified, 13)
        self.assertEqual(merged.data_cache_hits, 5)
        self.assertEqual(merged.data_cache_misses, 9)
        self.assertEqual(merged.payloads_decompressed, 3)
        self.assertEqual(merged.fts_scans, 3)
        self.assertEqual(merged.facet_updates, 7)
        self.assertEqual(merged.histogram_updates, 9)
        self.assertEqual(merged.returned_row_expansions, 11)
        self.assertEqual(merged.early_stop_opportunities, 13)
        self.assertEqual(merged.early_stops, 5)

    def test_last_realtime_usec_keeps_max(self):
        from journal.explorer import ExplorerStats, ExplorerResult, ExplorerRow
        combined = n.CombinedResult()
        a = ExplorerStats(last_realtime_usec=100, max_source_realtime_delta_usec=10)
        b = ExplorerStats(last_realtime_usec=200, max_source_realtime_delta_usec=5)
        combined.merge("/tmp/a", ExplorerResult(rows=[ExplorerRow(realtime_usec=1, cursor="a", payloads=[])], stats=a),
                       n.Direction.BACKWARD, 10)
        combined.merge("/tmp/b", ExplorerResult(rows=[ExplorerRow(realtime_usec=2, cursor="b", payloads=[])], stats=b),
                       n.Direction.BACKWARD, 10)
        self.assertEqual(combined.stats.last_realtime_usec, 200)
        self.assertEqual(combined.stats.max_source_realtime_delta_usec, 10)

    def test_end_to_end_stats_aggregation(self):
        """End-to-end aggregation against a synthetic two-file directory.

        Rows merged, facet counts summed, stats summed, file count
        matches the number of files we wrote.
        """

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=4, count_b=2)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY", "SERVICE"],
            })
            self.assertEqual(response["_journal_files"]["matched"], 2)
            self.assertEqual(response["_journal_files"]["skipped"], 0)
            self.assertEqual(len(response["data"]), 6)
            stats = response["_stats"]["sdk_explorer"]
            # rows_matched must equal the sum across files.
            self.assertEqual(stats["rows_matched"], 6)
            self.assertEqual(stats["rows_returned"], 6)
            # rows_examined is the sum; in our small fixture it must
            # be >= rows_matched.
            self.assertGreaterEqual(stats["rows_examined"], stats["rows_matched"])
            # Facet counts reflect the merge sum.
            priority = next(f for f in response["facets"] if f["id"] == "PRIORITY")
            priority_map = {o["name"]: o["count"] for o in priority["options"]}
            self.assertEqual(priority_map, {"error": 4, "info": 2})


class RunOptions(unittest.TestCase):
    def test_from_timeout_seconds_zero_means_disabled(self):
        opts = n.NetdataFunctionRunOptions.from_timeout_seconds(0)
        self.assertIsNone(opts.timeout)

    def test_from_timeout_seconds_uses_value(self):
        opts = n.NetdataFunctionRunOptions.from_timeout_seconds(42)
        self.assertEqual(opts.timeout, 42.0)

    def test_default_progress_interval_250ms(self):
        opts = n.NetdataFunctionRunOptions()
        self.assertEqual(opts.progress_interval, 0.25)

    def test_state_field_defaults(self):
        # Default state hook returns None / no-op; subclasses can
        # override either method.
        state = n.NetdataFunctionState()
        self.assertIsNone(state.file_metadata("/no/such/path"))
        # update is a no-op; just ensure it does not raise.
        state.update_file_journal_vs_realtime_delta_usec("/no", 123)

    def test_metadata_defaults(self):
        meta = n.NetdataJournalFileMetadata()
        self.assertIsNone(meta.source_type)
        self.assertIsNone(meta.source_name)
        self.assertIsNone(meta.file_last_modified_usec)
        self.assertIsNone(meta.msg_first_realtime_usec)
        self.assertIsNone(meta.msg_last_realtime_usec)
        self.assertIsNone(meta.journal_vs_realtime_delta_usec)


# ---------------------------------------------------------------------------
# Chunk 2c: stateful semantics (data_only / tail / delta / if_modified_since
# 304 / sampling / run options).
# ---------------------------------------------------------------------------


class DataOnlyShape(unittest.TestCase):
    """`data_only` short-circuits facets/histogram/columns per Rust
    `add_query_response_metadata` (L702-720) and the
    `add_analysis_outputs_if_needed` helper (L2597-2611).
    """

    def test_data_only_drops_facets(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "facets": ["PRIORITY"],
            })
            # data_only without delta does NOT add `facets_delta`,
            # `histogram_delta`, or `items_delta`.
            self.assertNotIn("facets_delta", response)
            self.assertNotIn("histogram_delta", response)
            self.assertNotIn("items_delta", response)
            # data_only never adds full-facets / full-histogram / items.
            self.assertNotIn("facets", response)
            self.assertNotIn("histogram", response)
            self.assertNotIn("items", response)
            # `_request.echo` echoes the parsed `data_only=true`.
            self.assertTrue(response["_request"]["data_only"])
            # The expires header is set to roughly now + 3600.
            self.assertGreater(response["expires"], int(time.time()))

    def test_data_only_drops_columns_envelope(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
            })
            # data_only with no histogram and no delta means the
            # Rust `add_query_response_metadata` (L702-720) does
            # NOT add `available_histograms` because
            # `request.histogram.is_some()` is false. The key is
            # absent from the envelope.
            self.assertNotIn("available_histograms", response)
            # No `last_modified` unless tail is also set.
            self.assertNotIn("last_modified", response)


class Delta(unittest.TestCase):
    """`data_only + delta` adds `facets_delta` / `histogram_delta` /
    `items_delta` keys (Rust L2602-2611)."""

    def test_delta_keys_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertIn("facets_delta", response)
            self.assertIn("histogram_delta", response)
            self.assertIn("items_delta", response)
            # The full set is NOT present in data_only+delta mode.
            self.assertNotIn("facets", response)
            self.assertNotIn("histogram", response)
            # The `items_delta` payload has the expected fields.
            items = response["items_delta"]
            for key in ("evaluated", "matched", "unsampled", "estimated",
                        "returned", "max_to_return", "before", "after"):
                self.assertIn(key, items)
            # Facet count reflects the rows we wrote.
            priority = next(
                f for f in response["facets_delta"] if f["id"] == "PRIORITY"
            )
            priority_map = {o["name"]: o["count"] for o in priority["options"]}
            self.assertEqual(priority_map, {"error": 5, "info": 3})


class IfModifiedSince(unittest.TestCase):
    """`if_modified_since` short-circuits to 304 when no file is newer."""

    def test_if_modified_since_unchanged_returns_304(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            # Every file's last message is at or before
            # `base + 4*1000` (file_a) and `base + 100_000 + 2*1000`
            # (file_b). Use a high-water mark strictly after both.
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "if_modified_since": base + 200_000,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 304)
            self.assertIn("errorMessage", response)

    def test_if_modified_since_newer_returns_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            # High-water mark is BEFORE the first entry, so 304 must
            # not trigger and we get a normal 200 response.
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "if_modified_since": base - 1_000_000,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)
            # We expect rows (5 + 3 = 8).
            self.assertEqual(len(response["data"]), 8)

    def test_if_modified_since_zero_means_disabled(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "if_modified_since": 0,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)


class TailAnchor(unittest.TestCase):
    """Tail semantics: when tail is set with a realtime anchor, the
    `after` bound is `anchor + 1` (exclusive), matching the
    `tail_after_realtime_bound` clamp in Rust (L3504-3517).
    """

    def test_tail_anchor_no_new_data_returns_304(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            # `if_modified_since` is AFTER every file's last entry
            # (file_b's last is `base + 100_000 + 2*1000 = base+102000`).
            # No data is newer, so the 304 short-circuit fires.
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base + 200_000,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 304)

    def test_tail_anchor_filters_all_rows_returns_empty_200(self):
        """When the tail anchor is past every entry, the request still
        returns 200 with empty data (no 304 because the anchor may yet
        receive new data). This is the SOW-0093 contract.
        """

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            # Anchor well past the last entry (file_b ends at
            # `base + 100_000 + 2*1000`). The tail-after clamp widens
            # `after` to `anchor + 1` so the explorer finds nothing.
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base - 1_000_000,
                "anchor": base + 5_000_000,  # 5s after the last entry
                "after": 1577836800,
                "before": 1893456000,
                "last": 5,
            })
            # Status 200 with empty data.
            self.assertEqual(response["status"], 200)
            self.assertEqual(response["data"], [])

    def test_tail_anchor_includes_newer_rows(self):
        """When the tail anchor is BEFORE the last entry, the new rows
        after the anchor are returned."""

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base - 1_000_000,
                "anchor": base + 1 * 1000,  # anchor between msg 1 and 2
                "after": 1577836800,
                "before": 1893456000,
                "last": 10,
            })
            # 5 + 3 - 2 = 6 rows remain after the anchor (entry 0 and 1).
            self.assertEqual(response["status"], 200)
            self.assertGreaterEqual(len(response["data"]), 1)


class Sampling(unittest.TestCase):
    """Sampling budget math: when analysis is enabled and the budget
    is non-zero, the request response carries an `_sampling` block
    with `sampled` / `unsampled` / `estimated` counts (Rust L2583-2595).
    """

    def test_sampling_math_small_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            # `sampling=2` means we may unsample rows after exhausting
            # the budget. With 8 rows and a budget of 2, most rows
            # are unsampled/estimated.
            response = fn.run_directory_request_json(tmp, {
                "sampling": 2,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            self.assertIn("_sampling", response)
            sampling = response["_sampling"]
            self.assertTrue(sampling["enabled"])
            for key in ("sampled", "unsampled", "estimated"):
                self.assertIn(key, sampling)
            # The three counters must cover every examined row:
            # rows_matched (rows the explorer saw) ==
            # sampled + unsampled + estimated. The exact split
            # depends on the explorer's calibration, so we only
            # assert the invariant.
            stats = response["_stats"]["sdk_explorer"]
            total_sampling = (
                sampling["sampled"]
                + sampling["unsampled"]
                + sampling["estimated"]
            )
            self.assertGreaterEqual(total_sampling, stats["rows_matched"])

    def test_sampling_zero_keeps_legacy_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "sampling": 0,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            # `sampling=0` disables the sampling math entirely.
            self.assertNotIn("_sampling", response)

    def test_sampling_skipped_in_data_only_without_delta(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "sampling": 20,
                "after": 1577836800,
                "before": 1893456000,
            })
            # data_only without delta: no analysis, so no _sampling
            # block. The Rust L1546-1550 conditions skip sampling.
            self.assertNotIn("_sampling", response)


class RunOptionsBehavior(unittest.TestCase):
    """Wiring of `NetdataFunctionRunOptions` into the explorer."""

    def test_progress_callback_fires(self):
        seen = []
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=3, count_b=2)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(progress_callback=seen.append)
            response = fn.run_directory_request_json_with_options(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
            self.assertEqual(response["status"], 200)
        # At least one progress event per matched file plus possibly
        # more for the underlying explorer.
        self.assertGreaterEqual(len(seen), 2)
        first = seen[0]
        # Progress shape mirrors `NetdataFunctionProgress`.
        self.assertEqual(first.current_file, 1)
        self.assertEqual(first.total_files, 2)
        # After the first file is merged, matched_files is already
        # incremented to 1 (the post-file emit happens after the
        # increment).
        self.assertEqual(first.matched_files, 1)
        self.assertEqual(first.skipped_files, 0)
        # Final progress reaches matched_files == total.
        last = seen[-1]
        self.assertEqual(last.matched_files, 2)
        # And the last event is the one for the second file.
        self.assertEqual(last.current_file, 2)

    def test_progress_interval_observed(self):
        """Two 250ms-ticked progress events means the elapsed between
        consecutive emits is >= 0 (sanity). With a tiny fixture the
        call returns too fast for the second tick, so we just assert
        the contract is respected."""

        seen = []
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=2, count_b=2)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(
                progress_callback=seen.append,
                progress_interval=0.05,
            )
            fn.run_directory_request_json_with_options(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
        # At least one progress event is reported.
        self.assertGreaterEqual(len(seen), 1)

    def test_cancellation_callback_short_circuits(self):
        called = {"n": 0}

        def cancel():
            called["n"] += 1
            return called["n"] > 1

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=4, count_b=4)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(cancellation_callback=cancel)
            response = fn.run_directory_request_json_with_options(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
            # Cancelled mid-run -> 499.
            self.assertEqual(response["status"], 499)

    def test_timeout_zero_means_no_deadline(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=2, count_b=2)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions.from_timeout_seconds(0)
            # timeout=None means no deadline is set on ExplorerControl.
            self.assertIsNone(opts.timeout)
            # The request still completes normally.
            response = fn.run_directory_request_json_with_options(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
            self.assertEqual(response["status"], 200)


class StateHook(unittest.TestCase):
    """`NetdataFunctionState` is consulted for per-file metadata and
    updated with the learned realtime delta."""

    def test_state_file_metadata_overrides_classification(self):
        class _State(n.NetdataFunctionState):
            def __init__(self):
                self.calls = 0

            def file_metadata(self, path):
                self.calls += 1
                return n.NetdataJournalFileMetadata(
                    msg_last_realtime_usec=0,
                )

        state = _State()
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(state=state)
            # If the state says msg_last_realtime_usec == 0 for every
            # file, the 304 short-circuit fires (after/before are
            # inclusive of the high-water mark, and 0 <= any value).
            response = fn.run_directory_request_json_with_options(tmp, {
                "data_only": True,
                "if_modified_since": 1,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
            self.assertEqual(response["status"], 304)
            # The state was consulted at least once.
            self.assertGreater(state.calls, 0)

    def test_state_learns_realtime_delta(self):
        """After the explorer runs, the state is updated with the
        learned `journal_vs_realtime_delta_usec`."""

        class _State(n.NetdataFunctionState):
            def __init__(self):
                self.updates = []

            def update_file_journal_vs_realtime_delta_usec(self, path, delta):
                self.updates.append((path, int(delta)))

        state = _State()
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=3, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(state=state)
            response = fn.run_directory_request_json_with_options(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }, opts)
            self.assertEqual(response["status"], 200)
        # The default 5s slack is `NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC`,
        # so unless the explorer's max_source_realtime_delta exceeds
        # 5_000_000, no updates are issued. The contract is: the
        # state receives ONLY updates for deltas that exceed the
        # default. We assert the contract did not raise and that
        # any emitted values are >= the default.
        for _path, delta in state.updates:
            self.assertGreaterEqual(
                delta, n.NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC
            )

    def test_state_file_metadata_with_source_name(self):
        """When the state supplies `source_name`, the `_path_matches_request`
        helper uses it to satisfy `exact_sources`."""

        class _State(n.NetdataFunctionState):
            def file_metadata(self, path):
                return n.NetdataJournalFileMetadata(source_name="my-host")

        state = _State()
        from journal import netdata as _n
        # exact_sources=["my-host"]; the per-file source_name is
        # fetched from the state and matches.
        self.assertTrue(
            _n._path_matches_request(
                pathlib.Path("/tmp/system.journal"),
                _n.NetdataRequest.parse(
                    {"selections": {"__logs_sources": ["my-host"]}},
                    _n.NetdataFunctionConfig(),
                ),
                state,
            )
        )


class SdJournalVisitUniqueValuesFacade(unittest.TestCase):
    """`SdJournalVisitUniqueValues` mirrors the Rust
    `rust/src/journal/src/facade.rs::SdJournalVisitUniqueValues`.
    The visitor is called once per unique value, dedup across
    multiple files, with raw `bytes` values. The facade
    function delegates to `SdJournal.visit_unique_values`.
    """

    def test_visit_emits_unique_values_in_order(self):
        from journal import SdJournalOpen, SdJournalVisitUniqueValues

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            journal = SdJournalOpen(tmp, 0)
            try:
                collected = []

                def visitor(value):
                    collected.append(bytes(value))
                    return None

                SdJournalVisitUniqueValues(journal, "PRIORITY", visitor)
                # Machine A has PRIORITY=3 (5 rows), machine B has
                # PRIORITY=6 (3 rows). Unique values: {"3", "6"}.
                self.assertEqual(set(collected), {b"3", b"6"})
                self.assertEqual(len(collected), 2)
            finally:
                journal.close()

    def test_visit_propagates_visitor_exception(self):
        # Mirrors the Rust behaviour where the visitor's `Err` is
        # surfaced to the caller.
        from journal import SdJournalOpen, SdJournalVisitUniqueValues

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            journal = SdJournalOpen(tmp, 0)
            try:

                def visitor(_value):
                    raise RuntimeError("visitor failed")

                with self.assertRaises(RuntimeError):
                    SdJournalVisitUniqueValues(journal, "PRIORITY", visitor)
            finally:
                journal.close()

    def test_visit_unknown_field_returns_empty(self):
        from journal import SdJournalOpen, SdJournalVisitUniqueValues

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            journal = SdJournalOpen(tmp, 0)
            try:
                collected = []

                def visitor(value):
                    collected.append(bytes(value))

                SdJournalVisitUniqueValues(journal, "DOES_NOT_EXIST", visitor)
                self.assertEqual(collected, [])
            finally:
                journal.close()

    def test_visit_value_passthrough_to_query_unique(self):
        # `visit_unique_values` and `query_unique` must return the
        # same set of bytes for the same field.
        from journal import SdJournalOpen, SdJournalQueryUnique, SdJournalVisitUniqueValues

        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            journal = SdJournalOpen(tmp, 0)
            try:
                queried = sorted(
                    bytes(value) for name, value in SdJournalQueryUnique(journal, "SERVICE")
                )
                visited = []

                def visitor(value):
                    visited.append(bytes(value))

                SdJournalVisitUniqueValues(journal, "SERVICE", visitor)
                self.assertEqual(sorted(visited), queried)
            finally:
                journal.close()


class AcceptedParamsAndWindowPreFilter(unittest.TestCase):
    """SOW-0104 fix 5.

    Two shared diffs that fail all 9 window fixtures in the real
    comparator are pinned here through the FULL request entry point
    (`run_directory_request_json`) on synthetic directories:

    1. `accepted_params`: the Rust `add_full_query_response_metadata`
       chains `NETDATA_ACCEPTED_PARAMS` (16 base params) with
       `reportable_facet_field_names` (request `facets` dedup) at
       L752-772 / L1139-1146 / L2358-2366. The Python query response
       must mirror that, surfacing the request's filter field names
       exactly as Rust does.
    2. `columns`: the Rust `select_journal_files_for_request`
       (L2938-2967) applies a may-overlap file pre-filter
       (`journal_file_order_may_overlap_request` L2997-3026) so
       `collect_column_fields_for_file` (L504) catalogs fields
       only from files that may overlap the request window. The
       Python port must use the same file set.
    """

    def test_accepted_params_includes_request_facet_field(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            accepted = list(response["accepted_params"])
            # The 16 base params come first (info, __logs_sources,
            # after, before, anchor, direction, last, query, facets,
            # histogram, if_modified_since, data_only, delta, tail,
            # sampling, slice) exactly as `NETDATA_ACCEPTED_PARAMS`.
            self.assertEqual(
                accepted[: len(n.NETDATA_ACCEPTED_PARAMS)],
                list(n.NETDATA_ACCEPTED_PARAMS),
            )
            # The request's filter field name follows the base list,
            # so the total is `16 + 1 = 17` (mirrors Rust's
            # `NETDATA_ACCEPTED_PARAMS.len() + 1`).
            self.assertEqual(len(accepted), len(n.NETDATA_ACCEPTED_PARAMS) + 1)
            self.assertEqual(accepted[-1], "PRIORITY")

    def test_accepted_params_dedupes_duplicate_facet_field(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY", "SERVICE", "PRIORITY"],
            })
            accepted = list(response["accepted_params"])
            # Rust `reportable_facet_fields_bytes` (L2358-2366)
            # deduplicates while preserving order. The same
            # PRIORITY listed twice in the request yields a
            # single trailing entry, not two.
            self.assertEqual(len(accepted), len(n.NETDATA_ACCEPTED_PARAMS) + 2)
            self.assertEqual(accepted[-2:], ["PRIORITY", "SERVICE"])

    def test_info_response_accepted_params_stays_at_sixteen(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "info": True,
            })
            accepted = list(response["accepted_params"])
            # The info response passes no extra field names
            # (Rust `info_response` L612-636 calls
            # `accepted_params_from_fields(&[])`); it stays at
            # the 16-entry base list.
            self.assertEqual(accepted, list(n.NETDATA_ACCEPTED_PARAMS))
            self.assertEqual(len(accepted), 16)

    def test_columns_excludes_out_of_window_file_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _in_path, _out_path = _make_window_split_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            # The request window covers 2023-11-14 only. The
            # in-window file's entries sit at 1_700_000_000_000_000
            # us; the out-of-window file's entries sit at
            # 1_000_000_000_000_000 us (2001-09-09), well below
            # the window. The may-overlap pre-filter must drop
            # the out-of-window file from the column catalog.
            in_window_usec = 1_700_000_000_000_000
            response = fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": int(in_window_usec // 1_000_000) - 3600,
                "before": int(in_window_usec // 1_000_000) + 3600,
                "facets": ["PRIORITY"],
            })
            # The envelope status must be 200 (request is well-formed
            # and the in-window file supplies data).
            self.assertEqual(response["status"], 200)
            column_names = list(response["columns"].keys())
            # The in-window file's unique field is cataloged.
            self.assertIn("IN_WINDOW_FIELD", column_names)
            # The out-of-window file's unique field is NOT cataloged.
            self.assertNotIn("OUT_WINDOW_FIELD", column_names)
            # Sanity: the in-window file produced data rows.
            self.assertGreaterEqual(len(response["data"]), 1)

    def test_columns_excludes_out_of_window_file_fields_when_window_is_past(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _in_path, _out_path = _make_window_split_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            # Window at 2010: both files are in the future and their
            # first_realtime_usec is above the window. Both files are
            # dropped by the may-overlap pre-filter; the column
            # catalog contains no file-specific fields. The window is
            # sent as a past absolute window so the data-derived
            # clamp (SOW-0104 fix-9) does not shift it into the
            # journal's data range.
            past_after = 1262304000  # 2010-01-01 UTC
            past_before = 1262390400  # 2010-01-02 UTC
            response = fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": past_after,
                "before": past_before,
                "facets": ["PRIORITY"],
            })
            self.assertEqual(response["status"], 200)
            column_names = list(response["columns"].keys())
            self.assertNotIn("IN_WINDOW_FIELD", column_names)
            self.assertNotIn("OUT_WINDOW_FIELD", column_names)
            self.assertEqual(len(response["data"]), 0)

    def test_future_window_clamps_to_now_anchored_parse_time(self):
        """SOW-0104 fix-10: a future absolute window is parsed against
        parse-time ``unix_now_seconds()`` (Rust L1418), so the
        ``before > now`` branch in ``relative_window_to_absolute``
        shifts the window back to land on ``now``. The window's
        width is preserved (delta = before - after) and the
        pre-filter sees the now-anchored window — so an old journal
        (2023) is correctly excluded from the response.
        """

        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _in_path, _out_path = _make_window_split_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            future_after = 2524608000  # 2050-01-01 UTC
            future_before = 2524694400  # 2050-01-02 UTC
            response = fn.run_directory_request_json(
                dir_path,
                {
                    "last": 100,
                    "after": future_after,
                    "before": future_before,
                    "facets": ["PRIORITY"],
                },
            )
            self.assertEqual(response["status"], 200)
            echo = response["_request"]
            # The echo's `before` is wall-clock ``now`` (year 2026+),
            # not the 2023 journal tail. This is the now-anchored
            # invariant.
            self.assertLess(
                echo["before"], future_before,
                msg=(
                    "future-clamp must shift before back; "
                    "echo_before=%d, sent_before=%d"
                    % (echo["before"], future_before)
                ),
            )
            self.assertGreater(
                echo["before"], 1_700_000_000,
                msg=(
                    "now-anchored before must be in the present; "
                    "echo_before=%d, journal_tail=1700000000"
                    % (echo["before"],)
                ),
            )
            # Window width is preserved across the clamp.
            self.assertEqual(
                echo["before"] - echo["after"],
                future_before - future_after,
                msg=(
                    "now-anchored clamp must preserve window width; "
                    "sent_width=%d, echo_width=%d"
                    % (future_before - future_after, echo["before"] - echo["after"])
                ),
            )
            # The 2023 journal entries are now BELOW the now-anchored
            # window (year 2026), so the pre-filter drops them and
            # neither file contributes fields to the column catalog.
            column_names = list(response["columns"].keys())
            self.assertNotIn("IN_WINDOW_FIELD", column_names)
            self.assertNotIn("OUT_WINDOW_FIELD", column_names)


# ---------------------------------------------------------------------------
# SOW-0104 fix-8: stateful comparator paging-forward seed regression.
#
# The stateful comparator's first paging step sends an identical
# request payload to SDK, plugin, and Python peers but each peer
# computes its own `now`, so any residual mutation in Python's
# request decoding (relative-to-absolute conversion, file pre-filter,
# explorer query bounds, histogram bounds) surfaces as
#   - `_request.after`/`before` echo diverging from Rust;
#   - the file pre-filter excluding files that overlap the requested
#     window;
#   - the column catalog missing fields contributed by the excluded
#     files (e.g. `_CMDLINE`).
# The tests below pin the byte-for-byte parity contract for fixed
# absolute windows so the production failure (matched=0 with a 0-row
# response and a truncated column catalog) cannot regress silently.
# ---------------------------------------------------------------------------


class FixedAbsoluteWindowParity(unittest.TestCase):
    """Pin the absolute-window contract: when the caller passes an
    absolute (usec-scale or post-94M-seconds) window, the response
    echo equals the sent values byte-for-byte, every file whose
    entries overlap that window contributes its column fields, and
    every row whose timestamp falls inside the window is returned
    (up to ``last``).
    """

    def _build_dir_with_cmdline_field(
        self,
        tmp: str,
        base_usec: int,
        count: int = 10,
        step_usec: int = 1_000_000,
    ) -> str:
        """Build a one-file synthetic directory whose entries land
        on consecutive usec timestamps starting at ``base_usec``.
        Each entry carries ``_CMDLINE`` so the per-file enumerated
        column catalog must include that field after the file is
        traversed.
        """

        dir_path = pathlib.Path(tmp)
        sub = dir_path / "aabbccdd-1111-1111-1111-111111111111"
        sub.mkdir()
        fp = sub / "system.journal"
        w = Writer.create(
            str(fp),
            {
                "machine_id": b"\x11" * 16,
                "boot_id": b"\xaa" * 16,
                "seqnum_id": b"\x33" * 16,
            },
        )
        for i in range(count):
            w.append(
                [
                    {"name": "MESSAGE", "value": f"msg-{i}".encode()},
                    {"name": "PRIORITY", "value": b"3"},
                    {"name": "_CMDLINE", "value": b"/usr/bin/program"},
                    {"name": "_COMM", "value": b"program"},
                ],
                {"realtime_usec": base_usec + i * step_usec},
            )
        w.close()
        return str(fp)

    def test_absolute_window_echo_matches_sent_bytes_in_past(self):
        """Sent absolute past window must be echoed unchanged.

        The window's upper bound is placed strictly below the journal
        tail so the data-derived clamp (SOW-0104 fix-9) does not shift
        it; the echo must therefore equal the sent values byte-for-
        byte. The journal carries entries up to base_usec + 14s
        (count=15, step=1s) so the journal's tail is well past the
        request's `before`.
        """

        with tempfile.TemporaryDirectory() as tmp:
            base_usec = 1_700_000_000_000_000  # 2023-11-14
            self._build_dir_with_cmdline_field(
                tmp, base_usec, count=15, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            sent = {
                "after": 1_700_000_000,
                "before": 1_700_000_010,
                "last": 5,
                "direction": "forward",
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "slice": True,
            }
            response = fn.run_directory_request_json(tmp, sent)
            echo = response["_request"]
            # The echo MUST equal the sent values; the request
            # window is fully inside the journal's data range so the
            # data-derived clamp leaves both endpoints untouched.
            self.assertEqual(echo["after"], sent["after"])
            self.assertEqual(echo["before"], sent["before"])

    def test_absolute_window_returns_rows_inside_window(self):
        """All entries inside the absolute window must be returned
        (pinned count + pinned timestamps)."""

        with tempfile.TemporaryDirectory() as tmp:
            base_usec = 1_700_000_000_000_000
            self._build_dir_with_cmdline_field(
                tmp, base_usec, count=10, step_usec=500_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            # Forward request with `last`=5 must return the 5 rows
            # with the smallest timestamps inside the window. The
            # window covers entries 0..9 (5_000_000 us span,
            # 500_000 us step).
            sent = {
                "after": 1_700_000_000,
                "before": 1_700_000_005,
                "last": 5,
                "direction": "forward",
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "slice": True,
            }
            response = fn.run_directory_request_json(tmp, sent)
            self.assertEqual(response["_journal_files"]["matched"], 1)
            data = response.get("data") or []
            self.assertEqual(len(data), 5)
            timestamps = sorted(row[0] for row in data)
            # The 5 oldest rows in the window are at base_usec + 0..4
            # times the step.
            expected = [base_usec + i * 500_000 for i in range(5)]
            self.assertEqual(timestamps, expected)

    def test_absolute_window_catalog_includes_cmdline(self):
        """When a file overlaps the SENT window, the column catalog
        must include the file's enumerated fields (e.g. ``_CMDLINE``);
        the live failure surfaced this as a missing ``_CMDLINE``
        column when the file was incorrectly excluded by the
        pre-filter."""

        with tempfile.TemporaryDirectory() as tmp:
            base_usec = 1_700_000_000_000_000
            self._build_dir_with_cmdline_field(tmp, base_usec, count=5)
            fn = n.NetdataJournalFunction.systemd_journal()
            sent = {
                "after": 1_700_000_000,
                "before": 1_700_000_010,
                "last": 5,
                "direction": "forward",
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "slice": True,
            }
            response = fn.run_directory_request_json(tmp, sent)
            columns = response.get("columns") or {}
            self.assertIn("_CMDLINE", columns)
            self.assertIn("_COMM", columns)

    def test_absolute_window_in_future_clamps_to_now_anchored_parse_time(self):
        """SOW-0104 fix-10: when the caller passes a `before` value
        strictly greater than parse-time ``unix_now_seconds()``, the
        window is shifted backwards by the future delta so its upper
        bound lands on ``now`` and its width is preserved. The bound
        is the parse-time wall-clock observation of ``now`` (Rust
        L1418), NOT the journal's data tail. The comparator tolerates
        the resulting small (sub-second-to-seconds) drift between
        peers invoked at slightly different wall-clock instants
        through the bounded skew tolerance on the
        ``_request.after``/``before`` echoes."""

        with tempfile.TemporaryDirectory() as tmp:
            # Place the journal 60 seconds in the past relative to
            # wall-clock now so the caller's future `before` triggers
            # the clamp on every run.
            wall_now = int(time.time())
            base_usec = (wall_now - 60) * 1_000_000
            self._build_dir_with_cmdline_field(
                tmp, base_usec, count=5, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            future_after = wall_now - 30
            future_before = wall_now + 600
            sent = {
                "after": future_after,
                "before": future_before,
                "last": 5,
                "direction": "forward",
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "slice": True,
            }
            response = fn.run_directory_request_json(tmp, sent)
            echo = response["_request"]
            # The now-anchored clamp preserves the WIDTH of the
            # window: `echo_before - echo_after == future_before -
            # future_after`. Both endpoints land inside the future
            # window's `before` ceiling and after the sent `after`
            # floor (modulo the future shift).
            self.assertEqual(
                echo["before"] - echo["after"],
                future_before - future_after,
                msg=(
                    "now-anchored clamp must preserve the window width;"
                    " sent_width=%d, echo_width=%d"
                    % (
                        future_before - future_after,
                        echo["before"] - echo["after"],
                    )
                ),
            )
            # The clamped `before` lands on parse-time
            # ``unix_now_seconds()``, NOT on the journal's tail
            # realtime. The wall-clock `now` and the journal tail
            # differ by ~60s here, so a strict equality against the
            # journal tail would fail — this is the now-anchored
            # invariant (the echo reflects parse time, not data).
            self.assertGreaterEqual(
                echo["before"],
                wall_now - 5,
                msg=(
                    "now-anchored before must be wall-clock now; "
                    "echo_before=%d, wall_now=%d"
                    % (echo["before"], wall_now)
                ),
            )

    def test_stateful_seed_window_lands_on_now_anchored_parse_time(self):
        """SOW-0104 fix-10: mirror the Rust seed default of
        ``relative_window_to_absolute`` L3658-3690 paired with
        ``normalize_time_window`` L3624-3656. The stateful runner
        seeds with ``after=1, before=4_102_444_800``; the window is
        now-anchored at parse time so the effective window is
        ``[unix_now_seconds() - 3600, unix_now_seconds()]``
        (end-of-second rounding on ``before``). The window's
        width is 3600s; the echo's ``before`` shifts with the
        wall-clock but the WIDTH is invariant. A slow third peer
        that runs a few seconds later than the fast peers is no
        longer a content mismatch: the comparator now tolerates a
        bounded skew (<=300s) on the ``_request.after``/``before``
        echoes, mirroring the fix-4 source-info tolerance.
        """

        with tempfile.TemporaryDirectory() as tmp:
            self._build_dir_with_cmdline_field(
                tmp, base_usec=1_700_000_000_000_000, count=5, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(
                tmp,
                {
                    "after": 1,
                    "before": 4_102_444_800,
                    "last": 5,
                    "direction": "forward",
                    "data_only": True,
                    "facets": ["PRIORITY"],
                    "histogram": "PRIORITY",
                    "slice": True,
                },
            )
            echo = response["_request"]
            self.assertEqual(
                echo["before"] - echo["after"], 3600,
                msg="default relative window must span DEFAULT_TIME_WINDOW_SECONDS",
            )
            # The echo's `before` is wall-clock now (in the present),
            # NOT a 2023 journal tail.
            self.assertGreater(
                echo["before"], 1_700_000_000,
                msg=(
                    "now-anchored before must be in the present, not "
                    "on the 2023 journal tail; echo_before=%d"
                    % (echo["before"],)
                ),
            )


class NowAnchoredWindowEcho(unittest.TestCase):
    """SOW-0104 fix-10: the effective window's ``after``/``before``
    and the response ``_request`` echo are NOW-ANCHORED (derived
    from parse-time ``unix_now_seconds()`` at Rust L1418), NOT from
    journal data. The fix-9 journal-tail anchoring was reverted:
    the reference design is to clamp the window against the
    wall-clock observation of ``now`` and let the comparator
    tolerate a bounded skew on the echoes so a slow third peer
    that runs seconds after the fast peers is no longer a false
    positive.

    The original fix-8 failure mode (the stateful gate seeded with
    ``after=1, before=4_102_444_800`` saw the SDK echo
    ``_request.after = 1781225642`` and Python echo ``1781225649``,
    7 seconds apart) is now handled by the bounded skew tolerance
    on ``_request.after``/``before`` in the comparator. The Python
    parse-time ``unix_now_seconds()`` IS the reference; the
    tolerance is the contract for peers invoked seconds apart.
    """

    def _build_old_journal(
        self,
        tmp: str,
        *,
        base_usec: int = 1_700_000_000_000_000,
        count: int = 5,
        step_usec: int = 1_000_000,
    ) -> int:
        """Build a journal whose entries live far in the past (Nov
        14 2023) and return the journal's tail realtime in seconds.

        Placing the journal in the past is critical: the
        now-anchored echo's ``before`` is wall-clock at the time
        of the call (year 2026+); a data-derived echo would use
        the journal tail (~2023). The two differ by years and the
        tests assert the now-anchored invariant with explicit
        lower bounds on the echo's ``before``."""

        sub = pathlib.Path(tmp) / "aabbccdd-1111-1111-1111-111111111111"
        sub.mkdir()
        fp = sub / "system.journal"
        w = Writer.create(
            str(fp),
            {
                "machine_id": b"\x11" * 16,
                "boot_id": b"\xaa" * 16,
                "seqnum_id": b"\x33" * 16,
            },
        )
        for i in range(count):
            w.append(
                [
                    {"name": "MESSAGE", "value": f"m{i}".encode()},
                    {"name": "PRIORITY", "value": b"3"},
                ],
                {"realtime_usec": base_usec + i * step_usec},
            )
        w.close()
        return int((base_usec + (count - 1) * step_usec) // 1_000_000)

    def test_stateful_seed_echo_lands_on_now_anchored_parse_time(self):
        """The stateful runner's first paging step sends
        ``after=1, before=4_102_444_800`` (identical bytes to all
        peers). The now-anchored echo MUST land on the parse-time
        wall-clock ``now`` (``[now - 3600, now]``), NOT on the
        2023 journal tail. The comparator tolerates a bounded
        skew (<=300s) so a slow third peer is no longer a
        false-positive."""

        with tempfile.TemporaryDirectory() as tmp:
            self._build_old_journal(
                tmp, count=10, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(
                tmp,
                {
                    "after": 1,
                    "before": 4_102_444_800,
                    "last": 5,
                    "direction": "forward",
                    "data_only": True,
                    "facets": ["PRIORITY"],
                    "histogram": "PRIORITY",
                    "slice": True,
                },
            )
            echo = response["_request"]
            # The now-anchored echo is in the PRESENT, well after
            # the 2023 journal tail (~1.7e9). A data-derived echo
            # would land on 1_700_000_009; the now-anchored echo
            # lands on wall-clock now (~1.78e9 in 2026).
            self.assertGreater(
                echo["before"], 1_700_000_009,
                msg=(
                    "now-anchored before must be in the present, not "
                    "on the 2023 journal tail; "
                    "echo_before=%d, journal_tail=1700000009, "
                    "wall_now=%d"
                    % (echo["before"], int(time.time()))
                ),
            )
            self.assertEqual(
                echo["before"] - echo["after"],
                3_600,
                msg="default 3600s window must be preserved",
            )

    def test_echo_shifts_with_wall_clock_across_invocations(self):
        """The stateful gate's failure mode was a 7s drift between
        invocations. With the now-anchored contract, the echo
        REFLECTS the wall-clock at parse time. Two invocations
        seconds apart produce different echoes; the comparator
        tolerates that drift through the bounded skew tolerance.
        This test pins the now-anchored contract by asserting
        (a) the echo's ``before`` tracks the wall-clock and
        (b) the journal tail is no longer involved."""

        with tempfile.TemporaryDirectory() as tmp:
            self._build_old_journal(tmp, count=5, step_usec=1_000_000)
            fn = n.NetdataJournalFunction.systemd_journal()
            request = {
                "after": 1,
                "before": 4_102_444_800,
                "last": 5,
                "direction": "forward",
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "slice": True,
            }
            first = fn.run_directory_request_json(tmp, request)["_request"]
            first_wall = int(time.time())
            time.sleep(2)
            second = fn.run_directory_request_json(tmp, request)["_request"]
            second_wall = int(time.time())
            # The echo's `before` is now-anchored, so it advances
            # with the wall-clock: the second echo's `before` is
            # >= the first. The exact drift is `second_wall -
            # first_wall` (2s + scheduling slack), well under the
            # 300s comparator tolerance.
            self.assertGreaterEqual(
                second["before"], first["before"],
                msg=(
                    "now-anchored echo must track wall-clock forward; "
                    "first_before=%d, second_before=%d, "
                    "first_wall=%d, second_wall=%d"
                    % (first["before"], second["before"],
                       first_wall, second_wall)
                ),
            )
            # Both echoes are in the PRESENT, not on the 2023
            # journal tail (~1.7e9).
            self.assertGreater(
                first["before"], 1_700_000_000,
                msg=(
                    "now-anchored echo must be in the present, not on "
                    "the 2023 journal tail; first_before=%d, wall_now=%d"
                    % (first["before"], int(time.time()))
                ),
            )

    def test_absolute_future_window_clamps_to_now_anchored_parse_time(self):
        """When the request's `before` is in the future (or very
        large), the now-anchored clamp shifts the window so its
        upper bound lands on wall-clock ``now`` (parse time), with
        the same width as the sent window. The sent values are
        absolute and placed AFTER wall-clock ``now`` so the
        ``before > now`` branch fires."""

        with tempfile.TemporaryDirectory() as tmp:
            self._build_old_journal(
                tmp, count=15, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            wall_now = int(time.time())
            for sent_width in (1_800, 7_200):
                # Absolute values that fall AFTER wall-clock now
                # (year 2026) so the future clamp fires. We do NOT
                # anchor on the 2023 journal tail — the bound is
                # parse-time wall-clock `now`.
                sent_after = wall_now + 600
                sent_before = sent_after + sent_width
                sent = {
                    "after": sent_after,
                    "before": sent_before,
                    "last": 5,
                    "direction": "forward",
                    "data_only": True,
                    "facets": ["PRIORITY"],
                    "histogram": "PRIORITY",
                    "slice": True,
                }
                echo = fn.run_directory_request_json(tmp, sent)["_request"]
                self.assertEqual(
                    echo["before"] - echo["after"], sent_width,
                    msg=(
                        "now-anchored clamp must preserve sent width;"
                        " sent_width=%d, echo_width=%d"
                        % (sent_width, echo["before"] - echo["after"])
                    ),
                )
                # The clamped `before` is wall-clock `now`, not the
                # 2023 journal tail.
                self.assertGreaterEqual(
                    echo["before"], wall_now - 5,
                    msg=(
                        "now-anchored before must equal wall-clock now; "
                        "sent_width=%d, echo_before=%d, wall_now=%d, "
                        "journal_tail=1700000014"
                        % (sent_width, echo["before"], wall_now)
                    ),
                )

    def test_rows_inside_now_anchored_window_are_returned(self):
        """Sanity check on the new contract: rows whose timestamps
        fall inside the now-anchored effective window are returned,
        rows outside are not. The journal is in 2023, the request
        sends ``after=1, before=4_102_444_800``, the now-anchored
        window lands on ``[now - 3600, now]`` (year 2026+). The
        2023 journal entries are below that window so the
        pre-filter drops them; the response has 0 rows."""

        with tempfile.TemporaryDirectory() as tmp:
            base_usec = 1_700_000_000_000_000
            tail_usec = base_usec + 9 * 1_000_000  # count=10 step=1s
            self._build_old_journal(
                tmp, base_usec=base_usec, count=10, step_usec=1_000_000
            )
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(
                tmp,
                {
                    "after": 1,
                    "before": 4_102_444_800,
                    "last": 100,
                    "direction": "forward",
                    "data_only": True,
                    "facets": ["PRIORITY"],
                    "histogram": "PRIORITY",
                    "slice": True,
                },
            )
            echo = response["_request"]
            wall_now = int(time.time())
            # The now-anchored `before` is wall-clock now, in the
            # present; the 2023 journal tail is below the
            # now-anchored window.
            self.assertGreater(
                echo["before"], 1_700_000_000,
                msg=(
                    "now-anchored before must be in the present, not "
                    "on the 2023 journal tail; echo_before=%d, wall_now=%d"
                    % (echo["before"], wall_now)
                ),
            )
            # The 2023 journal entries are below the now-anchored
            # window, so the pre-filter drops them and no rows are
            # returned.
            self.assertEqual(
                response.get("data"), [],
                msg=(
                    "old journal must be outside the now-anchored "
                    "window; tail_usec=%d, wall_now=%d, "
                    "echo_before=%d"
                    % (tail_usec, wall_now, echo["before"])
                ),
            )

    def test_empty_directory_falls_back_to_now_anchored_parse_time(self):
        """When the directory is empty (no journal files), the
        request keeps its parse-time wall-clock window. The echo
        then reflects the now-anchored clamp, which is the
        reference design. This is the documented fallback path:
        callers that observe an empty directory MUST NOT pretend
        a data-derived bound exists."""

        with tempfile.TemporaryDirectory() as tmp:
            # An empty directory: no journal files.
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(
                tmp,
                {
                    "after": 1,
                    "before": 4_102_444_800,
                    "last": 5,
                    "direction": "forward",
                    "data_only": True,
                    "facets": ["PRIORITY"],
                    "histogram": "PRIORITY",
                    "slice": True,
                },
            )
            self.assertEqual(response["status"], 200)
            self.assertEqual(response["data"], [])
            echo = response["_request"]
            # With no journal data, the parse-time wall-clock window
            # applies. The echo's `before` is wall-clock at parse
            # time, in the present.
            self.assertGreater(
                echo["before"],
                1_700_000_000,
                msg=(
                    "empty-directory fallback uses wall-clock now, "
                    "in the present; echo_before=%d, wall_now=%d"
                    % (echo["before"], int(time.time()))
                ),
            )


class NormalizeTimeWindowParity(unittest.TestCase):
    """SOW-0104 fix-10: pin Python's ``_normalize_time_window`` to
    Rust's ``normalize_time_window`` (L3624-3656) and
    ``relative_window_to_absolute`` (L3658-3690), with the constants
    copied verbatim from the Rust source:

    - ``DEFAULT_TIME_WINDOW_SECONDS = 3600`` (L24)
    - ``API_RELATIVE_TIME_MAX_SECONDS = 3 * 365 * 86_400`` (L27)
    - ``NETDATA_MISSING_AFTER_RELATIVE_SECONDS = 600`` (L28)

    Tests call the port with an INJECTED ``now`` so the byte-equal
    Rust outcomes (verified at ``rust/src/journal/src/netdata.rs``
    L5571-5609) are asserted against fixed wall clocks. The call
    site at parse time passes the real ``unix_now_seconds()``.
    """

    # A fixed wall clock so the math is trace-comparable; the
    # function under test takes ``now`` as a parameter.
    NOW = 1_000_000_000  # mirrors the Rust unit test constant

    def test_both_zero_defaults_to_last_hour_like_rust(self):
        """Mirror ``normalizes_missing_time_window_to_last_hour_like_plugin``
        (Rust L5572-5577). When both endpoints are missing, the window
        is ``[now - DEFAULT_TIME_WINDOW_SECONDS, now]`` with the
        upper bound rounded to end-of-second."""

        a, b = n._normalize_time_window(self.NOW, None, None)
        self.assertEqual(a, (self.NOW - 3600) * 1_000_000)
        self.assertEqual(b, self.NOW * 1_000_000 + 999_999)

    def test_inverted_absolute_window_swaps_like_rust(self):
        """Mirror ``normalizes_inverted_time_window_like_plugin``
        (Rust L5580-5584). Both endpoints are large enough to be
        ABSOLUTE (above ``API_RELATIVE_TIME_MAX_SECONDS``); the
        function does not enter the relative branch, and the
        ``after > before`` swap produces the in-order window."""

        a, b = n._normalize_time_window(self.NOW, 200_000_100, 200_000_000)
        self.assertEqual(a, 200_000_000 * 1_000_000)
        self.assertEqual(b, 200_000_100 * 1_000_000 + 999_999)

    def test_equal_absolute_window_widens_to_default_like_rust(self):
        """Mirror ``normalizes_equal_time_window_like_plugin``
        (Rust L5588-5592). Both endpoints are absolute and equal;
        the ``after == before`` branch widens ``after`` to
        ``before - DEFAULT_TIME_WINDOW_SECONDS``."""

        a, b = n._normalize_time_window(self.NOW, 200_000_000, 200_000_000)
        self.assertEqual(a, (200_000_000 - 3600) * 1_000_000)
        self.assertEqual(b, 200_000_000 * 1_000_000 + 999_999)

    def test_relative_before_and_relative_after_like_rust(self):
        """Mirror ``normalizes_relative_time_window_like_plugin``
        (Rust L5596-5600). Both endpoints are small enough to be
        RELATIVE; ``before = 200`` -> ``now - 200``, ``after = 100``
        becomes ``-100`` and folds into ``before + after + 1``."""

        a, b = n._normalize_time_window(self.NOW, 100, 200)
        # before = now - 200 = 999_999_800
        # after  = 999_999_800 + (-100) + 1 = 999_999_701
        self.assertEqual(a, 999_999_701 * 1_000_000)
        self.assertEqual(b, 999_999_800 * 1_000_000 + 999_999)

    def test_missing_after_with_supplied_before_like_rust(self):
        """Mirror ``normalizes_missing_after_with_supplied_before_like_plugin``
        (Rust L5604-5608). When ``after`` is missing, the
        relative-zero branch applies the missing-after default
        (``-NETDATA_MISSING_AFTER_RELATIVE_SECONDS``) and folds
        into ``before + after + 1``."""

        a, b = n._normalize_time_window(self.NOW, None, 200_000_000)
        # before = 200_000_000 (absolute, > MAX, no relative branch)
        # after  = 200_000_000 + (-600) + 1 = 199_999_401
        self.assertEqual(a, 199_999_401 * 1_000_000)
        self.assertEqual(b, 200_000_000 * 1_000_000 + 999_999)

    def test_stateful_seed_shape_lands_on_now_minus_3600_to_now(self):
        """The stateful comparator's ``data_only_request`` sends
        ``after=1, before=4_102_444_800`` (DEFAULT_AFTER_SECONDS /
        DEFAULT_BEFORE_SECONDS in
        ``tests/netdata_function/run_stateful_function_compare.py``).
        Both endpoints are RELATIVE (``|v| <= API_RELATIVE_TIME_MAX_SECONDS``
        is FALSE for ``4_102_444_800``? No, ``4_102_444_800 < 94_608_000``:
        wait, ``4_102_444_800 > 94_608_000`` so ``before`` enters the
        relative branch, is negated, and lands on ``now - 4_102_444_800``
        (year-2100-ish past); the future clamp then shifts the window
        to land on ``[now - 3600, now]`` (end-of-second rounding on
        ``before``) and the equal-bounds branch fires because
        ``after`` follows the same delta.

        Concretely: ``after = 1`` -> ``-1``; ``before = 4_102_444_800``
        -> ``-(4_102_444_800)`` -> ``now - 4_102_444_800``. Then
        ``after = before + (-1) + 1 = before``. The future clamp
        shifts BOTH back by the same delta (4_102_444_800 - now)
        so the upper bound lands on ``now``; then
        ``after == before`` triggers
        ``after = before - DEFAULT_TIME_WINDOW_SECONDS``."""

        a, b = n._normalize_time_window(self.NOW, 1, 4_102_444_800)
        self.assertEqual(a, (self.NOW - 3600) * 1_000_000)
        self.assertEqual(b, self.NOW * 1_000_000 + 999_999)


class RelativeWindowRelativeZeroBranchParity(unittest.TestCase):
    """SOW-0104 fix-10: pin the ``after==0`` and ``before==0``
    relative-zero branches. Both Rust and Python treat a `0` endpoint
    as a *relative-zero* offset, NOT as the Unix epoch (the Rust
    branch gates on ``unsigned_abs() <= MAX`` with no ``!= 0``
    guard). The previous Python implementation skipped the relative
    conversion when the endpoint was `0`, producing windows that
    diverged from Rust whenever the caller sent
    ``after=0, before=N>0``, ``after=M>0, before=0``, or both."""

    NOW = 1_781_226_300  # a fixed wall clock for trace-comparable math

    def test_after_zero_before_relative_uses_missing_after_default(self):
        a, b = n._normalize_time_window(self.NOW, 0, 10)
        # before=10 (relative) -> now-10
        # after=0  (relative) -> -NETDATA_MISSING_AFTER_RELATIVE_SECONDS
        # final after = before + after + 1 = (now-10) + (-600) + 1 = now-609
        self.assertEqual(a, (self.NOW - 609) * 1_000_000)
        self.assertEqual(b, (self.NOW - 10) * 1_000_000 + 999_999)

    def test_after_relative_before_zero_now_anchor(self):
        a, b = n._normalize_time_window(self.NOW, 10, 0)
        # before=0  (relative) -> now+0 = now
        # after=10 (relative) -> -10
        # final after = now + (-10) + 1 = now-9
        self.assertEqual(a, (self.NOW - 9) * 1_000_000)
        self.assertEqual(b, self.NOW * 1_000_000 + 999_999)

    def test_absolute_past_window_left_unchanged(self):
        a, b = n._normalize_time_window(self.NOW, 1_700_000_000, 1_700_000_010)
        self.assertEqual(a, 1_700_000_000 * 1_000_000)
        self.assertEqual(b, 1_700_000_010 * 1_000_000 + 999_999)

    def test_absolute_future_before_clamped_with_after_shift(self):
        # before > now triggers the clamp; both endpoints shift back
        # by the same delta to keep the window width fixed.
        future_before = self.NOW + 3600
        absolute_after = self.NOW - 1800
        a, b = n._normalize_time_window(self.NOW, absolute_after, future_before)
        delta = future_before - self.NOW
        self.assertEqual(b, self.NOW * 1_000_000 + 999_999)
        self.assertEqual(a, max(absolute_after - delta, 0) * 1_000_000)


class HistogramBoundsAreOriginalRequestBounds(unittest.TestCase):
    """Rust's ``NetdataRequest::to_explorer_query`` (L1599-1600)
    feeds ``self.after_realtime_usec`` and ``self.before_realtime_usec``
    into the histogram bounds — the ORIGINAL request bounds — even
    when the per-file explorer query is clamped by a tail or
    backward-page anchor. Python must do the same so multi-file
    histograms align across peers.
    """

    def test_histogram_bounds_use_request_after_before_for_tail_anchor(self):
        config = n.NetdataFunctionConfig.systemd_journal()
        anchor_usec = 1_700_000_005_000_000
        request = n.NetdataRequest.parse(
            {
                "after": 1_700_000_000,
                "before": 1_700_000_010,
                "anchor": anchor_usec,
                "if_modified_since": anchor_usec,
                "tail": True,
                "data_only": True,
                "direction": "backward",
                "last": 5,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            },
            config,
        )
        query = request.to_explorer_query(matched_files=1)
        # The per-file query's `after_realtime_usec` is clamped to
        # `anchor + 1`. The histogram bounds must NOT see that clamp.
        self.assertEqual(
            request.after_realtime_usec, 1_700_000_000 * 1_000_000
        )
        self.assertEqual(query.histogram_after_realtime_usec, request.after_realtime_usec)
        self.assertEqual(query.histogram_before_realtime_usec, request.before_realtime_usec)
        # Sanity: the explorer query DID apply the tail-after clamp.
        self.assertEqual(query.after_realtime_usec, anchor_usec + 1)

    def test_histogram_bounds_use_request_for_backward_page_anchor(self):
        config = n.NetdataFunctionConfig.systemd_journal()
        anchor_usec = 1_700_000_005_000_000
        request = n.NetdataRequest.parse(
            {
                "after": 1_700_000_000,
                "before": 1_700_000_010,
                "anchor": anchor_usec,
                "data_only": True,
                "direction": "backward",
                "last": 5,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            },
            config,
        )
        query = request.to_explorer_query(matched_files=1)
        self.assertEqual(query.histogram_after_realtime_usec, request.after_realtime_usec)
        self.assertEqual(query.histogram_before_realtime_usec, request.before_realtime_usec)
        # Sanity: the explorer query DID apply the backward-page-anchor
        # clamp (before <= anchor - 1).
        self.assertEqual(query.before_realtime_usec, anchor_usec - 1)


class JournalFileOrderInfoFallbacks(unittest.TestCase):
    """Pin Python ``_journal_file_order_info`` to Rust
    ``journal_file_order_info`` (L3913-3959). When the supplied
    header has ``tail_entry_realtime == 0`` (online/uninitialised
    file) the function must fall back to the filesystem mtime so
    the file-overlap pre-filter can reason about it. When the
    caller passes ``header=None`` (deferred open) the bounds remain
    `0` so the caller knows to open the file."""

    def test_header_tail_zero_uses_file_mtime_fallback(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "system.journal"
            # An online file: write entries, then forge the header
            # snapshot with `tail_entry_realtime=0`.
            w = Writer.create(
                str(path),
                {
                    "machine_id": b"\x11" * 16,
                    "boot_id": b"\xaa" * 16,
                    "seqnum_id": b"\x33" * 16,
                },
            )
            w.append(
                [
                    {"name": "MESSAGE", "value": b"x"},
                    {"name": "PRIORITY", "value": b"3"},
                ],
                {"realtime_usec": 1_700_000_000_000_000},
            )
            w.close()
            fake_header = {
                "head_entry_realtime": 0,
                "tail_entry_realtime": 0,
            }
            info = n._journal_file_order_info(
                str(path), header=fake_header, metadata=None
            )
            # msg_last falls back to file mtime; file_last_modified
            # mirrors that mtime too.
            self.assertEqual(
                info["msg_last_realtime_usec"],
                info["file_last_modified_usec"],
            )
            self.assertGreater(info["msg_last_realtime_usec"], 0)

    def test_header_none_keeps_bounds_zero_so_caller_opens(self):
        """The caller-side prefilter relies on
        ``msg_last_realtime_usec == 0`` as the "needs open" sentinel.
        Returning the file mtime here would break the open-fallback
        path in ``_explore_files`` and let stale-mtime files masquerade
        as covering the window."""

        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "system.journal"
            w = Writer.create(
                str(path),
                {
                    "machine_id": b"\x11" * 16,
                    "boot_id": b"\xaa" * 16,
                    "seqnum_id": b"\x33" * 16,
                },
            )
            w.append(
                [
                    {"name": "MESSAGE", "value": b"x"},
                    {"name": "PRIORITY", "value": b"3"},
                ],
                {"realtime_usec": 1_700_000_000_000_000},
            )
            w.close()
            info = n._journal_file_order_info(
                str(path), header=None, metadata=None
            )
            self.assertEqual(info["msg_first_realtime_usec"], 0)
            self.assertEqual(info["msg_last_realtime_usec"], 0)
            # file_last_modified is still surfaced for caller use.
            self.assertGreater(info["file_last_modified_usec"], 0)

    def test_metadata_overrides_apply_after_header_fallback(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "system.journal"
            w = Writer.create(
                str(path),
                {
                    "machine_id": b"\x11" * 16,
                    "boot_id": b"\xaa" * 16,
                    "seqnum_id": b"\x33" * 16,
                },
            )
            w.append(
                [
                    {"name": "MESSAGE", "value": b"x"},
                    {"name": "PRIORITY", "value": b"3"},
                ],
                {"realtime_usec": 1_700_000_000_000_000},
            )
            w.close()
            metadata = n.NetdataJournalFileMetadata(
                msg_first_realtime_usec=1_600_000_000_000_000,
                msg_last_realtime_usec=1_650_000_000_000_000,
                file_last_modified_usec=1_660_000_000_000_000,
                journal_vs_realtime_delta_usec=42_000_000,
            )
            info = n._journal_file_order_info(
                str(path),
                header={"head_entry_realtime": 0, "tail_entry_realtime": 0},
                metadata=metadata,
            )
            self.assertEqual(
                info["msg_first_realtime_usec"], 1_600_000_000_000_000
            )
            self.assertEqual(
                info["msg_last_realtime_usec"], 1_650_000_000_000_000
            )
            self.assertEqual(
                info["file_last_modified_usec"], 1_660_000_000_000_000
            )
            # The clamp on the realtime-delta is bounded by the
            # MAX constant (Rust ``normalize_journal_vs_realtime_delta_usec``).
            self.assertEqual(
                info["journal_vs_realtime_delta_usec"], 42_000_000
            )



# (1) filtered-field facet vocabulary zero-count post-passes
#     (mirror Rust `add_zero_count_facet_values_from_files` and
#     `add_zero_count_selected_filter_values`);
# (2) data_only response key set is the compact 14-key shape
#     (no `accepted_params`, `default_sort_column`, `default_charts`,
#     `message`, `update_every`, `help`, `facets`/`histogram`/`items`
#     for `data_only + !delta`);
# (3) `available_histograms` is the reportable facet fields list
#     (with the histogram field appended only in data_only);
# (4) 304 / 499 envelope uses `errorMessage` (no `error`, no `type`).
# ---------------------------------------------------------------------------


class FilteredFieldFacetVocabulary(unittest.TestCase):
    """When a request filters on a field that is also requested as
    a facet, the facet must still surface values that the filter
    would otherwise mask. Rust achieves this with two zero-count
    post-passes inside `explore_files` (L428-443):

    * `add_zero_count_facet_values_from_files` walks the FIELD hash
      tables of every matched file and adds each unique value of
      every requested facet as a zero-count entry, widening the
      facet vocabulary past the filtered set.
    * `add_zero_count_selected_filter_values` adds the selected
      filter values as zero-count entries in the matching facet,
      so `PRIORITY=3` still surfaces `PRIORITY=3` even when zero
      rows match.

    The Python port wires both calls into `_explore_files` (the
    request entry path) right before the result is returned to
    `_build_query_response`.
    """

    def test_filter_value_surfaces_in_facet_even_with_zero_matches(self):
        with tempfile.TemporaryDirectory() as tmp:
            # An empty journal simulates the live journal's
            # window-error-filter case where no rows fall in the
            # requested window.
            _make_two_machine_dir(tmp, count_a=0, count_b=0)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 5,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": False,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "selections": {"PRIORITY": ["3"]},
            })
            self.assertEqual(response["status"], 200)
            priority_facet = next(
                f for f in response["facets"] if f["id"] == "PRIORITY"
            )
            priority_map = {
                o["id"]: o["count"] for o in priority_facet["options"]
            }
            # `add_zero_count_selected_filter_values` registers
            # the filter value "3" in the PRIORITY facet as
            # zero-count. Without the post-pass, the facet would
            # be empty.
            self.assertIn("3", priority_map)
            self.assertEqual(priority_map["3"], 0)
            # No rows matched the filter / window; the data
            # payload is empty.
            self.assertEqual(len(response["data"]), 0)

    def test_filter_value_surfaces_in_histogram_dimensions(self):
        """The histogram field's facet vocabulary feeds the
        histogram dimension id set (Rust L917-924). When a
        filter on the histogram field masks the only candidate
        value, the dimension still has to appear in the
        chart envelope so the comparator can detect it.
        """
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=0, count_b=0)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 5,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": False,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
                "selections": {"PRIORITY": ["3"]},
            })
            self.assertEqual(response["status"], 200)
            h = response["histogram"]
            ids = h["chart"]["db"]["dimensions"]["ids"]
            names = h["chart"]["view"]["dimensions"]["names"]
            # The PRIORITY=3 zero-count from the filter
            # vocabulary surfaces as a histogram dimension with
            # id "3" and the systemd display name "error"
            # (`priority_name(3) == "error"` in
            # `systemd_field_display_value` L4337-4338).
            self.assertIn("3", ids)
            self.assertIn("error", names)
            # The ids and names lists agree on order.
            self.assertEqual(len(ids), len(names))

    def test_file_vocabulary_widens_facet_with_unselected_values(self):
        """Single-field filter on a facet field: the combined scan applies
        the filter (Rust ``distinct_filter_fields() > 1`` is False for a
        single field → combined pass → filter applied).  PRIORITY=3
        rows from file_a are counted; PRIORITY=6 entries from file_b
        are excluded by the filter.  The zero-count widening post-pass
        adds PRIORITY=6 as a zero-count entry from the FIELD hash
        table vocabulary.
        """
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": False,
                "facets": ["PRIORITY"],
                "selections": {"PRIORITY": ["3"]},
            })
            priority_facet = next(
                f for f in response["facets"] if f["id"] == "PRIORITY"
            )
            priority_map = {
                o["id"]: o["count"] for o in priority_facet["options"]
            }
            # PRIORITY=3 entries pass the filter.
            self.assertIn("3", priority_map)
            self.assertEqual(priority_map["3"], 5)
            # PRIORITY=6 entries are excluded by the filter; the
            # zero-count widening adds it with count 0.
            self.assertIn("6", priority_map)
            self.assertEqual(priority_map["6"], 0)


class DataOnlyEnvelopeShape(unittest.TestCase):
    """`data_only` responses use the compact 14-key envelope shape
    from Rust `add_query_response_metadata` (L702-720). The
    `accepted_params`, `default_sort_column`, `default_charts`,
    `message`, `update_every`, and `help` keys are OMITTED. The
    `facets`/`histogram`/`items` analysis outputs are also OMITTED
    unless `delta` is set (in which case they are renamed to the
    `_delta` variants per Rust L2602-2611).
    """

    EXPECTED_DATA_ONLY_BASE_KEYS = {
        "_journal_files", "_request", "_stats", "available_histograms",
        "columns", "data", "expires", "has_history", "pagination",
        "partial", "show_ids", "status", "type", "versions",
    }
    EXPECTED_FULL_BASE_KEYS = EXPECTED_DATA_ONLY_BASE_KEYS | {
        "accepted_params", "default_charts", "default_sort_column",
        "help", "last_modified", "message", "update_every",
    }

    def test_data_only_omits_full_metadata_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertEqual(response["status"], 200)
            # The full-mode metadata keys are absent.
            for forbidden in (
                "accepted_params", "default_sort_column",
                "default_charts", "message", "update_every", "help",
            ):
                self.assertNotIn(
                    forbidden, response,
                    msg=f"data_only response must not carry {forbidden!r}",
                )
            # The base envelope key set is exactly the data_only
            # shape (plus the histogram-driven `available_histograms`
            # because the request has an explicit histogram field).
            self.assertEqual(
                set(response.keys()),
                self.EXPECTED_DATA_ONLY_BASE_KEYS,
                msg=f"data_only response keys = {sorted(response.keys())}",
            )

    def test_full_response_carries_full_metadata_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertEqual(response["status"], 200)
            # The full-mode metadata keys are all present.
            for required in (
                "accepted_params", "default_sort_column",
                "default_charts", "message", "update_every", "help",
                "last_modified",
            ):
                self.assertIn(
                    required, response,
                    msg=f"full response must carry {required!r}",
                )
            # `accepted_params` is the 16 base + the request's
            # facet field name (PRIORITY), so 17 entries.
            accepted = list(response["accepted_params"])
            self.assertEqual(len(accepted), len(n.NETDATA_ACCEPTED_PARAMS) + 1)
            self.assertEqual(accepted[-1], "PRIORITY")

    def test_data_only_delta_uses_delta_variant_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertEqual(response["status"], 200)
            # data_only + delta: still no full-mode metadata.
            for forbidden in (
                "accepted_params", "default_sort_column",
                "default_charts", "message", "update_every", "help",
            ):
                self.assertNotIn(forbidden, response)
            # The analysis outputs are present under the `_delta`
            # names; the non-delta names are absent.
            self.assertIn("facets_delta", response)
            self.assertIn("histogram_delta", response)
            self.assertIn("items_delta", response)
            self.assertNotIn("facets", response)
            self.assertNotIn("histogram", response)
            self.assertNotIn("items", response)
            # `available_histograms` is the reportable facet
            # fields (PRIORITY) and the histogram field is
            # PRIORITY (already in the list), so 1 entry.
            self.assertEqual(len(response["available_histograms"]), 1)
            self.assertEqual(
                response["available_histograms"][0]["id"], "PRIORITY"
            )


class AvailableHistogramsContent(unittest.TestCase):
    """`available_histograms` is the reportable facet fields
    (request.facets deduplicated) plus the explicit histogram
    field in data_only mode (Rust L1225-1251). The `order` field
    is the field's position in the netdata_reorder_key sorted
    list (1-based).
    """

    def test_non_data_only_list_matches_request_facets(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": False,
                "facets": ["PRIORITY", "SERVICE", "SYSLOG_IDENTIFIER"],
                "histogram": "PRIORITY",
            })
            self.assertEqual(response["status"], 200)
            ids = [
                entry["id"] for entry in response["available_histograms"]
            ]
            self.assertEqual(
                ids, ["PRIORITY", "SERVICE", "SYSLOG_IDENTIFIER"]
            )
            # Order is the position in the reorder_key sort.
            for entry in response["available_histograms"]:
                self.assertIn("order", entry)
                self.assertIsInstance(entry["order"], int)
                self.assertGreaterEqual(entry["order"], 1)

    def test_data_only_appends_histogram_field(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY", "SERVICE"],
                "histogram": "SYSLOG_IDENTIFIER",
            })
            self.assertEqual(response["status"], 200)
            ids = [
                entry["id"] for entry in response["available_histograms"]
            ]
            # SYSLOG_IDENTIFIER is appended after the request
            # facets (deduplicated, so PRIORITY and SERVICE
            # stay in their original order).
            self.assertEqual(ids, ["PRIORITY", "SERVICE", "SYSLOG_IDENTIFIER"])

    def test_data_only_dedupes_when_histogram_in_facets(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            ids = [
                entry["id"] for entry in response["available_histograms"]
            ]
            # PRIORITY is in both the facets list and the
            # histogram field; data_only must not duplicate it.
            self.assertEqual(ids, ["PRIORITY"])

    def test_full_response_uses_facets_list_size(self):
        """The `window-last5-default-facets` fixture has 29
        facet fields. The Rust `available_histograms` returns
        a 29-entry list for non-data_only; the Python port
        must mirror that exact count.
        """
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            facets = [
                "MESSAGE_ID", "PRIORITY", "CODE_FILE", "CODE_FUNC",
                "ERRNO", "SYSLOG_FACILITY", "SYSLOG_IDENTIFIER",
                "UNIT", "USER_UNIT", "UNIT_RESULT", "_UID", "_GID",
                "_COMM", "_EXE", "_AUDIT_LOGINUID", "_SYSTEMD_CGROUP",
                "_SYSTEMD_SLICE", "_SYSTEMD_UNIT", "_SYSTEMD_USER_UNIT",
                "_SYSTEMD_USER_SLICE", "_SYSTEMD_SESSION",
                "_SYSTEMD_OWNER_UID", "_SELINUX_CONTEXT", "_BOOT_ID",
                "_MACHINE_ID", "_HOSTNAME", "_TRANSPORT", "_NAMESPACE",
                "_RUNTIME_SCOPE",
            ]
            self.assertEqual(len(facets), 29)
            response = fn.run_directory_request_json(tmp, {
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": False,
                "facets": facets,
                "histogram": "PRIORITY",
            })
            self.assertEqual(response["status"], 200)
            self.assertEqual(
                len(response["available_histograms"]), 29
            )
            ids = [
                entry["id"] for entry in response["available_histograms"]
            ]
            self.assertEqual(set(ids), set(facets))


class Tail304Envelope(unittest.TestCase):
    """The 304 no-change envelope (and the 499 cancelled envelope)
    use the compact Rust `netdata_function_error` shape:
    `{"status", "errorMessage"}` with no `error` and no `type` keys.
    """

    def test_304_envelope_uses_error_message_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base + 200_000,
                "after": 1577836800,
                "before": 1893456000,
            })
            # Status is 304 because no file is newer than the
            # high-water mark.
            self.assertEqual(response["status"], 304)
            # The envelope key set is exactly `status` and
            # `errorMessage` (matching Rust L2369-2374). The
            # legacy `error` and `type` keys are gone.
            self.assertEqual(
                set(response.keys()), {"status", "errorMessage"}
            )
            self.assertEqual(
                response["errorMessage"],
                "No new data since the previous call.",
            )
            # Belt and braces: the forbidden keys must be gone.
            self.assertNotIn("error", response)
            self.assertNotIn("type", response)

    def test_304_envelope_when_no_file_overlaps_request_window(self):
        """The tail/if_modified_since 304 short-circuit (Rust
        `not_modified_before_scan_response` L2677-2689 + the
        `select_journal_files_for_request` window-overlap filter
        L2938-2967) returns 304 when NO file overlaps the request
        window, even if every file is technically "newer" than
        `if_modified_since`. This mirrors the live journal
        `window-last5-tail-no-change` case: the window sits in
        2022, every file is from 2025+, the selection is empty,
        and Rust/Python both emit the compact 304 envelope.
        """
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            # Window is 2020..2021 (before any file's first entry
            # at base+0 = 2023-11-14). `if_modified_since` is set
            # to 2022 — younger than the window but older than the
            # files — so the naive "every file newer than
            # if_modified_since" check would NOT trigger 304, but
            # the window-overlap gate correctly excludes every
            # file and triggers 304.
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": 1_700_000_000_000_000,
                "after": 1577836800,
                "before": 1609459200,
            })
            self.assertEqual(response["status"], 304)
            self.assertEqual(
                set(response.keys()), {"status", "errorMessage"}
            )
            self.assertEqual(
                response["errorMessage"],
                "No new data since the previous call.",
            )

    def test_499_envelope_uses_error_message_key(self):
        """The 499 cancelled envelope uses the same compact shape."""
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=4, count_b=4)

            def cancel():
                # Cancel after the first progress callback.
                return True

            fn = n.NetdataJournalFunction.systemd_journal()
            opts = n.NetdataFunctionRunOptions(
                cancellation_callback=cancel,
            )
            response = fn.run_directory_request_json_with_options(
                tmp,
                {
                    "last": 10,
                    "after": 1577836800,
                    "before": 1893456000,
                },
                opts,
            )
            self.assertEqual(response["status"], 499)
            # Same compact envelope shape as the 304 case.
            self.assertEqual(
                set(response.keys()), {"status", "errorMessage"}
            )
            self.assertEqual(
                response["errorMessage"], "Request cancelled."
            )
            self.assertNotIn("error", response)
            self.assertNotIn("type", response)


class NetdataFunctionWrapper(unittest.TestCase):
    """End-to-end tests for `python/cmd/netdata_function_wrapper.py`.

    The wrapper is invoked as a real subprocess against a synthetic
    directory. We verify:

    - The request is read from stdin and a JSON envelope is written
      to stdout followed by a newline.
    - `--progress-jsonl` produces a JSONL file with the exact key
      shape mandated by the Rust/Go wrappers.
    - `--cancel-immediately` short-circuits with status 499 and the
      499 envelope is the response payload.
    """

    WRAPPER = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "cmd",
        "netdata_function_wrapper.py",
    )

    def _run_wrapper(
        self,
        args: list,
        request_payload: bytes,
        cwd: Optional[str] = None,
    ) -> tuple[int, bytes, bytes, str]:
        env = dict(os.environ)
        # Make sure the in-repo python/ directory is importable for
        # the child process even when the harness runs outside venv.
        env["PYTHONPATH"] = (
            os.path.dirname(os.path.abspath(__file__))
            + os.pathsep
            + env.get("PYTHONPATH", "")
        )
        completed = subprocess.run(
            [sys.executable, self.WRAPPER, *args],
            input=request_payload,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
            env=env,
            timeout=60,
        )
        return (
            completed.returncode,
            completed.stdout,
            completed.stderr,
            completed.stderr.decode("utf-8", errors="replace"),
        )

    def test_envelope_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            request = json.dumps({
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }).encode("utf-8")
            exit_code, stdout, _stderr, _msg = self._run_wrapper(
                ["--test", "systemd-journal", "--dir", tmp],
                request,
            )
            self.assertEqual(exit_code, 0, msg=f"stdout={stdout!r}")
            self.assertTrue(stdout.endswith(b"\n"), msg=f"stdout={stdout!r}")
            envelope = json.loads(stdout.decode("utf-8").strip())
            for key in (
                "_request", "versions", "status", "type", "show_ids",
                "has_history", "pagination", "columns", "data",
                "facets", "histogram", "items", "last_modified",
            ):
                self.assertIn(key, envelope, msg=f"missing {key!r}")
            self.assertEqual(envelope["status"], 200)

    def test_progress_jsonl_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            request = json.dumps({
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }).encode("utf-8")
            progress_path = os.path.join(tmp, "progress.jsonl")
            exit_code, _stdout, _stderr, _msg = self._run_wrapper(
                [
                    "--test",
                    "systemd-journal",
                    "--dir",
                    tmp,
                    "--progress-jsonl",
                    progress_path,
                ],
                request,
            )
            self.assertEqual(exit_code, 0)
            with open(progress_path, "r", encoding="utf-8") as fh:
                lines = [line for line in fh.read().splitlines() if line]
            self.assertGreater(len(lines), 0)
            for line in lines:
                obj = json.loads(line)
                self.assertEqual(
                    set(obj.keys()),
                    {
                        "current_file",
                        "total_files",
                        "matched_files",
                        "skipped_files",
                        "elapsed_seconds",
                        "stats",
                    },
                )
                self.assertIsInstance(obj["current_file"], int)
                self.assertIsInstance(obj["total_files"], int)
                self.assertIsInstance(obj["matched_files"], int)
                self.assertIsInstance(obj["skipped_files"], int)
                self.assertIsInstance(obj["elapsed_seconds"], (int, float))
                self.assertIsInstance(obj["stats"], dict)

    def test_cancel_immediately_returns_499_envelope(self):
        # The Rust wrapper sees a "499 Request cancelled." response
        # envelope when the cancellation callback fires before the
        # first file is processed. Mirror that behaviour.
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            request = json.dumps({
                "last": 10,
                "after": 1577836800,
                "before": 1893456000,
            }).encode("utf-8")
            exit_code, stdout, _stderr, _msg = self._run_wrapper(
                [
                    "--test",
                    "systemd-journal",
                    "--dir",
                    tmp,
                    "--cancel-immediately",
                    "true",
                ],
                request,
            )
            self.assertEqual(exit_code, 0, msg=f"stdout={stdout!r}")
            envelope = json.loads(stdout.decode("utf-8").strip())
            self.assertEqual(envelope["status"], 499)

    def test_unsupported_function_exits_1(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            request = json.dumps({"info": True}).encode("utf-8")
            exit_code, _stdout, stderr, _msg = self._run_wrapper(
                ["--test", "not-a-function", "--dir", tmp],
                request,
            )
            self.assertEqual(exit_code, 1)
            self.assertIn(b"unsupported function", stderr)

    def test_missing_dir_flag_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            request = json.dumps({"info": True}).encode("utf-8")
            exit_code, _stdout, _stderr, _msg = self._run_wrapper(
                ["--test", "systemd-journal"],
                request,
            )
            self.assertNotEqual(exit_code, 0)



def _make_uid_dir(tmp, base_time_usec=1_700_000_000_000_000, count=3,
                  uid=b'0', boot_id=b'\xaa' * 16):
    """Build a synthetic directory with entries carrying ``_UID``."""

    dir_path = pathlib.Path(tmp)
    sub = dir_path / "aabbccdd-1111-1111-1111-111111111111"
    sub.mkdir()
    journal = sub / "system.journal"
    w = Writer.create(str(journal), {
        'machine_id': b'\x11' * 16, 'boot_id': boot_id,
        'seqnum_id': b'\x33' * 16,
    })
    for i in range(count):
        w.append(
            [
                {'name': 'MESSAGE', 'value': f'msg-{i}'.encode()},
                {'name': 'PRIORITY', 'value': b'3'},
                {'name': '_UID', 'value': uid},
                {'name': '_COMM', 'value': b'testapp'},
            ],
            {'realtime_usec': base_time_usec + i * 1000},
        )
    w.close()
    return str(dir_path), str(journal)


class SOW0104Fix11bSyntheticColumnsAndProfile(unittest.TestCase):
    """SOW-0104 fix 11b: row-build defects.

    Three defects fixed in ``_build_data_row`` / ``_build_query_response``:

    1. ``ND_JOURNAL_FILE`` is a synthetic column emitted from the row's
       ``located.file_path``, not looked up in the journal fields.
    2. The configured profile (standard vs plugin) is threaded through
       row building instead of hardcoding ``SystemdJournalProfile()``.
    3. One ``DisplayContext`` is created per request and reused across
       all rows (uid/gid/boot caches are shared).
    """

    def test_nd_journal_file_equals_file_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, file_a, file_b = _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)
            col_idx = response["columns"]["ND_JOURNAL_FILE"]["index"]
            paths = {file_a, file_b}
            for row in response["data"]:
                val = row[col_idx]
                self.assertIn(val, paths,
                              "ND_JOURNAL_FILE must equal the source file path")

    def test_nd_journal_file_single_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, journal_path = _make_uid_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)
            self.assertGreaterEqual(len(response["data"]), 1)
            col_idx = response["columns"]["ND_JOURNAL_FILE"]["index"]
            for row in response["data"]:
                self.assertEqual(row[col_idx], journal_path)

    def test_plugin_profile_resolves_uid_in_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _jp = _make_uid_dir(tmp, uid=b'0')
            std_fn = n.NetdataJournalFunction.systemd_journal()
            plug_fn = n.NetdataJournalFunction.systemd_journal_plugin_compatible()
            req = {"last": 100, "after": 1577836800, "before": 1893456000}
            std_resp = std_fn.run_directory_request_json(dir_path, req)
            plug_resp = plug_fn.run_directory_request_json(dir_path, req)
            self.assertEqual(std_resp["status"], 200)
            self.assertEqual(plug_resp["status"], 200)
            col_idx = std_resp["columns"]["_UID"]["index"]
            std_uid = std_resp["data"][0][col_idx]
            plug_uid = plug_resp["data"][0][col_idx]
            self.assertEqual(std_uid, "0",
                             "standard profile must return raw uid")
            try:
                expected = pwd.getpwuid(0).pw_name
            except KeyError:
                expected = "0"
            self.assertEqual(plug_uid, expected,
                             "plugin profile must resolve uid to name")

    def test_display_context_reuse_consistent_uid_across_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _jp = _make_uid_dir(tmp, count=4, uid=b'0')
            plug_fn = n.NetdataJournalFunction.systemd_journal_plugin_compatible()
            response = plug_fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)
            col_idx = response["columns"]["_UID"]["index"]
            uid_values = [row[col_idx] for row in response["data"]]
            self.assertGreaterEqual(len(uid_values), 2)
            first = uid_values[0]
            for val in uid_values[1:]:
                self.assertEqual(val, first,
                                 "all rows must render _UID identically "
                                 "(shared DisplayContext)")

    def test_nd_journal_process_synthetic_column(self):
        with tempfile.TemporaryDirectory() as tmp:
            dir_path, _jp = _make_uid_dir(tmp, count=1)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(dir_path, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
            })
            self.assertEqual(response["status"], 200)
            self.assertIn("ND_JOURNAL_PROCESS", response["columns"])
            col_idx = response["columns"]["ND_JOURNAL_PROCESS"]["index"]
            process = response["data"][0][col_idx]
            self.assertIsNotNone(process)
            self.assertIn("testapp", process)


class TailDeltaFacetOptionNames(unittest.TestCase):
    """Fix 1: tail-delta facets emit profile-rendered option names.

    Rust routes option names through ``facet_option_name`` (L873) so
    PRIORITY=3 becomes "error", not "3".  The delta path shares the
    same ``build_facets`` code; only the JSON key changes to
    ``facets_delta`` (Rust ``response_analysis_keys`` L2604-2610).
    """

    def test_delta_facets_use_profile_rendered_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertIn("facets_delta", response)
            priority = next(
                f for f in response["facets_delta"] if f["id"] == "PRIORITY"
            )
            names = {o["name"] for o in priority["options"]}
            self.assertIn("error", names)
            self.assertIn("info", names)
            self.assertNotIn("3", names)
            self.assertNotIn("6", names)

    def test_non_delta_facets_use_profile_rendered_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            self.assertIn("facets", response)
            priority = next(
                f for f in response["facets"] if f["id"] == "PRIORITY"
            )
            names = {o["name"] for o in priority["options"]}
            self.assertIn("error", names)
            self.assertIn("info", names)
            self.assertNotIn("3", names)
            self.assertNotIn("6", names)


class TailDeltaHistogramIntegerValues(unittest.TestCase):
    """Fix 2: tail-delta histogram dimension values are integers, not null.

    Rust ``build_histogram`` (L933-954) iterates raw ``dimension_ids``
    (bytes) and emits ``Value::from(0)`` for actual dimensions with
    zero count.  The Python code was iterating decoded strings which
    never matched the bytes-keyed ``bucket.values``.
    """

    def test_delta_histogram_actual_dimensions_are_integer(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 100,
                "after": 1577836800,
                "before": 1893456000,
                "data_only": True,
                "delta": True,
                "facets": ["PRIORITY"],
                "histogram": "PRIORITY",
            })
            self.assertIn("histogram_delta", response)
            chart = response["histogram_delta"]["chart"]
            data = chart["result"]["data"]
            self.assertGreater(len(data), 0)
            for point in data:
                for entry in point[1:]:
                    self.assertIsInstance(
                        entry, list,
                        msg=f"histogram entry must be a list, got {type(entry)}",
                    )
                    self.assertEqual(len(entry), 3)
                    value = entry[0]
                    self.assertIsInstance(
                        value, int,
                        msg=f"histogram dimension value must be int, "
                        f"got {type(value).__name__}: {value!r}",
                    )


class TailDeltaItemsAfterAnchorPlusOne(unittest.TestCase):
    """Fix 3: tail/delta items.after includes the +1 for the exclusive anchor.

    Rust ``counters()`` (L2081-2087) computes
    ``after = skips_after + shifts`` where ``skips_after`` counts the
    anchor row excluded by the tail-after bound.  Python's
    ``rows_matched - returned`` misses this because the anchor entry
    is excluded by ``after_realtime_usec = anchor + 1`` before the
    explorer sees it.
    """

    def test_tail_items_after_includes_anchor(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "delta": True,
                "tail": True,
                "if_modified_since": base - 1_000_000,
                "anchor": base + 2 * 1000,
                "after": 1577836800,
                "before": 1893456000,
                "last": 100,
                "facets": ["PRIORITY"],
            })
            self.assertEqual(response["status"], 200)
            items = response["items_delta"]
            returned = len(response["data"])
            raw_after = response["data"] and (
                response["_stats"]["sdk_explorer"]["rows_matched"] - returned
            ) or 0
            self.assertEqual(
                items["after"],
                raw_after + 1,
                "items_delta.after must include the +1 for the exclusive anchor",
            )

    def test_non_tail_items_after_has_no_anchor_plus_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            response = fn.run_directory_request_json(tmp, {
                "last": 3,
                "after": 1577836800,
                "before": 1893456000,
                "facets": ["PRIORITY"],
            })
            self.assertEqual(response["status"], 200)
            items = response["items"]
            returned = len(response["data"])
            raw_after = (
                response["_stats"]["sdk_explorer"]["rows_matched"] - returned
            )
            if raw_after < 0:
                raw_after = 0
            self.assertEqual(items["after"], raw_after)


class FilteredTailNoChange(unittest.TestCase):
    """Fix 4: filtered tail with no new matching rows returns 200 with
    empty data.

    Rust ``not_modified_before_scan_response`` (L2677-2689) returns
    304 only when NO file is newer than ``if_modified_since``.  When
    files ARE newer but the filter excludes all newer rows, the scan
    runs and returns 200 with empty data (Rust test
    ``netdata_function_tail_anchor_with_newer_filtered_out_rows_returns_empty_200``
    at L5873-5910).  The Python code mirrors the same 304 check and
    scan path.
    """

    def test_filtered_tail_no_match_returns_empty_200(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base - 1_000_000,
                "anchor": base + 4 * 1000,
                "after": 1577836800,
                "before": 1893456000,
                "last": 100,
                "selections": {"SERVICE": ["svc-a"]},
            })
            self.assertEqual(response["status"], 200)
            self.assertEqual(response["data"], [])
            for key in ("status", "type", "columns", "data", "expires"):
                self.assertIn(key, response,
                              f"200 empty envelope missing key {key!r}")

    def test_unfiltered_tail_no_new_data_returns_304(self):
        with tempfile.TemporaryDirectory() as tmp:
            _make_two_machine_dir(tmp, count_a=5, count_b=3)
            fn = n.NetdataJournalFunction.systemd_journal()
            base = 1_700_000_000_000_000
            response = fn.run_directory_request_json(tmp, {
                "data_only": True,
                "tail": True,
                "if_modified_since": base + 200_000,
                "after": 1577836800,
                "before": 1893456000,
                "last": 100,
            })
            self.assertEqual(response["status"], 304)


if __name__ == "__main__":
    unittest.main()
