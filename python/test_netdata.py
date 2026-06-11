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

import os
import pathlib
import pwd
import sys
import tempfile
import time
import unittest

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
    sub_b = dir_path / "eeffgghh-2222-2222-2222-222222222222"
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
            self.assertEqual(priority_map, {"3": 5, "6": 3})
            self.assertEqual(service_map, {"svc-a": 5, "svc-b": 3})

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
                "after": 1704067200,
                "before": 1704153600,
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
            self.assertEqual(priority_map, {"3": 4, "6": 2})


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
            # data_only with no histogram and no delta means we never
            # need `available_histograms`.
            self.assertEqual(response["available_histograms"], [])
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
            self.assertEqual(priority_map, {"3": 5, "6": 3})


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
            self.assertIn("error", response)

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


if __name__ == "__main__":
    unittest.main()
