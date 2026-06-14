#!/usr/bin/env python3
"""Focused tests for `python/journal/explorer.py`.

Ports the intent of the Rust unit tests in
`rust/src/journal/src/explorer.rs` (cfg(test) block at L3414+) and the
Go tests in `go/journal/explorer_test.go` for the Python
explorer surface. Synthetic fixtures are built with the in-repo
Python Writer (synthetic identities only; never host journal).
"""

import os
import sys
import tempfile
import time

# Make `journal.*` importable from this script's parent dir.
HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

from journal import (  # noqa: E402
    DEFAULT_HISTOGRAM_TARGET_BUCKETS,
    DEFAULT_TIME_SLACK_USEC,
    EXPLORER_CONTROL_CHECK_EVERY_ROWS,
    EXPLORER_PROGRESS_INTERVAL_MS,
    Direction,
    ExplorerAnchor,
    ExplorerAnchorKind,
    ExplorerComparison,
    ExplorerControl,
    ExplorerError,
    ExplorerFieldMode,
    ExplorerFilter,
    ExplorerFtsPattern,
    ExplorerHistogram,
    ExplorerHistogramBucket,
    ExplorerProgress,
    ExplorerQuery,
    ExplorerResult,
    ExplorerRow,
    ExplorerSampling,
    ExplorerStats,
    ExplorerStopReason,
    ExplorerStrategy,
    ExplorerUnsupported,
    FileReader,
    Writer,
    Log,
)
from journal.explorer import _ExplorerSamplingState, _combined_sampling_decision  # noqa: E402


# ----------------------------------------------------------------------------
# Fixture helpers.
# ----------------------------------------------------------------------------


def _make_writer(path, machine_id=b'\xaa' * 16, boot_id=b'\xbb' * 16, seqnum_id=b'\xcc' * 16):
    return Writer.create(
        path,
        {
            'machine_id': machine_id,
            'boot_id': boot_id,
            'seqnum_id': seqnum_id,
        },
    )


def _write_simple_entries(path, entries):
    """Write a list of (realtime_usec, [(field, value), ...]) entries to a file."""

    w = _make_writer(path)
    for realtime, fields in entries:
        w.append(
            [{'name': k, 'value': v} for k, v in fields],
            {'realtime_usec': int(realtime)},
        )
    w.close()


def _write_many_alternating(path, count):
    """Write `count` entries alternating SERVICE=even / SERVICE=odd."""

    w = _make_writer(path)
    for i in range(count):
        service = b'even' if i % 2 == 0 else b'odd'
        message = 'row-{0}'.format(i).encode('ascii')
        w.append(
            [
                {'name': 'MESSAGE', 'value': message},
                {'name': 'SERVICE', 'value': service},
            ],
            {'realtime_usec': 1_700_000_000_000_000 + i},
        )
    w.close()


def _sample_facets(result, field):
    return result.facets.get(field, {})


def _facet_count(result, field, value):
    return _sample_facets(result, field).get(value, 0)


def _histogram_total_for_value(histogram, value):
    if histogram is None:
        return 0
    total = 0
    for bucket in histogram.buckets:
        total += bucket.values.get(value, 0)
    return total


# ----------------------------------------------------------------------------
# Defaults (mirrors Rust ExplorerQuery::default() at L116-132).
# ----------------------------------------------------------------------------


def test_explorer_query_defaults_match_rust():
    q = ExplorerQuery()
    assert q.after_realtime_usec is None
    assert q.before_realtime_usec is None
    assert q.anchor.kind == ExplorerAnchorKind.AUTO
    assert q.anchor.realtime_usec == 0
    assert q.direction == Direction.FORWARD
    assert q.limit == 200
    assert q.filters == []
    assert q.facets == []
    assert q.histogram is None
    assert q.histogram_after_realtime_usec is None
    assert q.histogram_before_realtime_usec is None
    assert q.histogram_target_buckets == DEFAULT_HISTOGRAM_TARGET_BUCKETS
    assert q.fts_terms == []
    assert q.fts_patterns == []
    assert q.fts_negative_patterns == []
    assert q.field_mode == ExplorerFieldMode.FIRST_VALUE
    assert q.exclude_facet_field_filters is True
    assert q.use_source_realtime is True
    assert q.realtime_slack_usec == DEFAULT_TIME_SLACK_USEC
    assert q.stop_when_rows_full is False
    assert q.stop_when_rows_full_check_every == 1
    assert q.sampling is None
    assert q.debug_collect_column_fields_by_row_traversal is False


# ----------------------------------------------------------------------------
# Builders (return self for chaining, like Rust's consuming builder).
# ----------------------------------------------------------------------------


def test_explorer_query_builders_return_self():
    q = ExplorerQuery()
    r1 = q.with_filter('SERVICE', ['api'])
    r2 = q.with_facet('PRIORITY')
    r3 = q.with_histogram('PRIORITY')
    r4 = q.with_fts_pattern('alpha')
    r5 = q.with_fts_negative_pattern('boom')
    assert r1 is q and r2 is q and r3 is q and r4 is q and r5 is q
    assert isinstance(q.filters[0], ExplorerFilter)
    assert q.filters[0].field == b'SERVICE'
    assert q.filters[0].values == [b'api']
    assert q.facets == [b'PRIORITY']
    assert q.histogram == b'PRIORITY'
    assert len(q.fts_terms) == 2
    assert q.fts_terms[0].negative is False
    assert q.fts_terms[1].negative is True
    assert q.fts_patterns == [b'alpha']
    assert q.fts_negative_patterns == [b'boom']


# ----------------------------------------------------------------------------
# ExplorerFtsPattern semantics (mirrors Rust L145-179).
# ----------------------------------------------------------------------------


def test_fts_substring_splits_on_star_and_drops_empty():
    p = ExplorerFtsPattern.substring(b'a*b*', negative=False)
    assert p.parts == [b'a', b'b']
    p2 = ExplorerFtsPattern.substring(b'**hello**world**', negative=False)
    assert p2.parts == [b'hello', b'world']


def test_fts_substring_matches_case_insensitive_in_order_with_advancement():
    p = ExplorerFtsPattern.substring(b'ERROR*INFO', negative=False)
    # Case-insensitive ASCII fold.
    assert p.matches(b'error happened, then info later')
    # Out-of-order: "info" before "error" -> no match.
    assert not p.matches(b'info before error')
    # No "INFO" anywhere -> no match.
    assert not p.matches(b'error happened only')
    # Three-part pattern: each part must follow in order.
    p2 = ExplorerFtsPattern.substring(b'BOOT*KERNEL*SHUTDOWN', negative=False)
    assert p2.matches(b'system boot kernel panic shutdown')
    # Kernel before boot -> no match.
    assert not p2.matches(b'kernel before boot shutdown')


def test_fts_substring_empty_parts_match_all_empty_value_matches_none():
    p = ExplorerFtsPattern.substring(b'**', negative=False)
    assert p.parts == []
    # Empty parts => match all.
    assert p.matches(b'anything')
    # Empty value never matches.
    assert not p.matches(b'')


# ----------------------------------------------------------------------------
# Filter / facet / histogram correctness on a synthetic file (mirrors
# Go TestExplorerTraversalFacetsHistogramFiltersAndRows L11-46).
# ----------------------------------------------------------------------------


def test_explorer_traversal_filters_facets_histogram_and_rows():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'simple.journal')
        entries = [
            (1_000, [('MESSAGE', b'alpha'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
            (2_000, [('MESSAGE', b'beta'), ('SERVICE', b'api'), ('PRIORITY', b'5')]),
            (3_000, [('MESSAGE', b'gamma'), ('SERVICE', b'worker'), ('PRIORITY', b'6')]),
            (4_000, [('MESSAGE', b'error alpha'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
            (5_000, [('MESSAGE', b'debug'), ('SERVICE', b'worker'), ('PRIORITY', b'4')]),
        ]
        _write_simple_entries(path, entries)
        reader = FileReader.open(path)
        try:
            q = (
                ExplorerQuery()
                .with_filter('SERVICE', ['api'])
                .with_facet('PRIORITY')
                .with_facet('SERVICE')
                .with_histogram('PRIORITY')
                .with_fts_pattern('alpha')
            )
            q.use_source_realtime = False
            q.limit = 10
            result = reader.explore(q)
            # Two rows match: SERVICE=api AND FTS contains "alpha"
            # (realtime 1_000 "alpha" and realtime 4_000 "error alpha").
            assert len(result.rows) == 2
            assert _facet_count(result, b'PRIORITY', b'6') == 2
            assert _facet_count(result, b'SERVICE', b'api') == 2
            assert _histogram_total_for_value(result.histogram, b'6') == 2
            assert result.stats.rows_returned == 2
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Index strategy matches Traversal on Index-supported shapes (mirrors Go
# TestExplorerIndexStrategyMatchesTraversalForAllValues / Rust L3949).
# Note: this test exercises the no-filter path, which is the simplest
# Index-supported shape. A separate Compare-mode test below pins the
# filtered path so the indexed candidate-set walk cannot silently
# drift from Traversal.
# ----------------------------------------------------------------------------


def test_explorer_index_strategy_matches_traversal_for_all_values():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'index.journal')
        entries = [
            (1_000, [('MESSAGE', b'one'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
            (2_000, [('MESSAGE', b'two'), ('SERVICE', b'api'), ('PRIORITY', b'5')]),
            (3_000, [('MESSAGE', b'three'), ('SERVICE', b'worker'), ('PRIORITY', b'6')]),
            (4_000, [('MESSAGE', b'four'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
        ]
        _write_simple_entries(path, entries)
        reader = FileReader.open(path)
        try:
            q = (
                ExplorerQuery()
                .with_facet('PRIORITY')
                .with_histogram('PRIORITY')
            )
            q.use_source_realtime = False
            q.field_mode = ExplorerFieldMode.ALL_VALUES
            q.limit = 2
            traversal = reader.explore_with_strategy(q, ExplorerStrategy.TRAVERSAL)
            indexed = reader.explore_with_strategy(q, ExplorerStrategy.INDEX)
            assert len(traversal.rows) == len(indexed.rows)
            assert traversal.facets == indexed.facets
            for t, i in zip(traversal.rows, indexed.rows):
                assert t.realtime_usec == i.realtime_usec
                assert t.payloads == i.payloads
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Compare strategy runs both and verifies equality (mirrors Go
# TestExplorerIndexCompareMatchesTraversal L75-105). The Compare path
# is restricted to single-file readers and to queries that produce the
# same output via Traversal and Index; we exercise the no-filter shape
# here so the two strategies are required to agree.
# ----------------------------------------------------------------------------


def test_explorer_compare_strategy_verifies_equality_and_fills_comparison():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'compare.journal')
        entries = [
            (1_000, [('MESSAGE', b'one'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
            (2_000, [('MESSAGE', b'two'), ('SERVICE', b'api'), ('PRIORITY', b'5')]),
            (3_000, [('MESSAGE', b'three'), ('SERVICE', b'worker'), ('PRIORITY', b'6')]),
            (4_000, [('MESSAGE', b'four'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
        ]
        _write_simple_entries(path, entries)
        reader = FileReader.open(path)
        try:
            # No filter (the Index path's candidate walk currently
            # does not apply the reader's filter via step()). The
            # Compare strategy verifies that Traversal and Index
            # produce the same output for a query shape both
            # strategies can serve.
            q = (
                ExplorerQuery()
                .with_facet('PRIORITY')
                .with_histogram('PRIORITY')
            )
            q.use_source_realtime = False
            q.field_mode = ExplorerFieldMode.ALL_VALUES
            q.limit = 2
            result = reader.explore_with_strategy(q, ExplorerStrategy.COMPARE)
            assert isinstance(result.comparison, ExplorerComparison)
            assert result.comparison.traversal_duration >= 0.0
            assert result.comparison.index_duration >= 0.0
            # All 4 entries contribute to the facet count (no filter).
            assert _facet_count(result, b'PRIORITY', b'6') == 3
            assert _facet_count(result, b'PRIORITY', b'5') == 1
            assert _histogram_total_for_value(result.histogram, b'6') == 3
            assert _histogram_total_for_value(result.histogram, b'5') == 1
        finally:
            reader.close()


def test_explorer_index_compare_matches_traversal_with_filters():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'compare-filter.journal')
        entries = [
            (1_000, [('MESSAGE', b'one'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
            (2_000, [('MESSAGE', b'two'), ('SERVICE', b'api'), ('PRIORITY', b'5')]),
            (3_000, [('MESSAGE', b'three'), ('SERVICE', b'worker'), ('PRIORITY', b'6')]),
            (4_000, [('MESSAGE', b'four'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
        ]
        _write_simple_entries(path, entries)
        reader = FileReader.open(path)
        try:
            q = (
                ExplorerQuery()
                .with_filter('SERVICE', ['api'])
                .with_facet('PRIORITY')
                .with_histogram('PRIORITY')
            )
            q.use_source_realtime = False
            q.field_mode = ExplorerFieldMode.ALL_VALUES
            q.limit = 10
            result = reader.explore_with_strategy(q, ExplorerStrategy.COMPARE)
            assert isinstance(result.comparison, ExplorerComparison)
            assert result.stats.rows_matched == 3
            assert _facet_count(result, b'PRIORITY', b'6') == 2
            assert _facet_count(result, b'PRIORITY', b'5') == 1
            assert _histogram_total_for_value(result.histogram, b'6') == 2
            assert _histogram_total_for_value(result.histogram, b'5') == 1
        finally:
            reader.close()


def test_explorer_index_collect_rows_does_not_use_linear_offset_lookup():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'index-offset-map.journal')
        _write_many_alternating(path, 64)
        reader = FileReader.open(path)
        try:
            original = reader._index_for_entry_offset

            def forbidden(_entry_offset):
                raise AssertionError(
                    '_index_for_entry_offset must not be used in indexed row collection'
                )

            reader._index_for_entry_offset = forbidden
            try:
                q = ExplorerQuery().with_facet('SERVICE').with_histogram('SERVICE')
                q.use_source_realtime = False
                q.field_mode = ExplorerFieldMode.ALL_VALUES
                q.limit = 10
                result = reader.explore_with_strategy(q, ExplorerStrategy.INDEX)
                assert len(result.rows) == 10
            finally:
                reader._index_for_entry_offset = original
        finally:
            reader.close()


def test_explorer_sampling_skips_and_estimates_rows_before_expansion():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'sampling.journal')
        count = 600
        base = 1_700_000_000_000_000
        _write_many_alternating(path, count)
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery().with_facet('SERVICE').with_histogram('SERVICE')
            q.use_source_realtime = False
            q.limit = 5
            q.after_realtime_usec = base
            q.before_realtime_usec = base + count
            q.histogram_target_buckets = 2
            q.sampling = ExplorerSampling(
                budget=20,
                matched_files=1,
                file_head_realtime_usec=base,
                file_tail_realtime_usec=base + count - 1,
                file_entries=count,
            )
            result = reader.explore(q)
            assert result.stats.sampling_sampled > 0
            assert result.stats.sampling_unsampled > 0
            assert result.stats.rows_unsampled > 0 or result.stats.rows_estimated > 0
            assert (
                _histogram_total_for_value(result.histogram, b'[unsampled]')
                + _histogram_total_for_value(result.histogram, b'[estimated]')
            ) > 0
            assert result.stats.rows_examined < count
        finally:
            reader.close()


def test_explorer_sampling_seqnum_estimate_clamps_progress_above_one():
    q = ExplorerQuery().with_facet('SERVICE')
    q.after_realtime_usec = 1_700_000_000_000_000
    q.before_realtime_usec = 1_700_000_001_000_000
    q.direction = Direction.BACKWARD
    q.limit = 5
    q.sampling = ExplorerSampling(
        budget=20,
        matched_files=1,
        file_head_realtime_usec=q.after_realtime_usec,
        file_tail_realtime_usec=q.before_realtime_usec,
        file_head_seqnum=1,
        file_tail_seqnum=100,
        file_entries=100,
    )
    state = _ExplorerSamplingState.for_query(q, None)
    assert state is not None
    state.per_file_sampled = 10
    assert state._estimate_remaining_rows_by_seqnum(99) == 90


def test_explorer_control_candidate_row_callback_feeds_sampling_decision():
    class FakeSampling:
        def __init__(self):
            self.calls = []

        def decide(self, commit_realtime, seqnum, candidate):
            self.calls.append((commit_realtime, seqnum, candidate))
            return None

    q = ExplorerQuery()
    q.limit = 0
    control = ExplorerControl()
    fake = FakeSampling()
    control.set_sampling_state(fake)
    control.set_candidate_row_callback(lambda realtime_usec: realtime_usec == 123)

    assert _combined_sampling_decision(q, [], 123, 7, None, control) is None
    assert fake.calls == [(123, 7, True)]


def test_explorer_index_rejects_first_value_semantics():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'index-reject.journal')
        _write_simple_entries(
            path,
            [(1_000, [('MESSAGE', b'one'), ('SERVICE', b'api')])],
        )
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery().with_facet('SERVICE')
            # field_mode is FIRST_VALUE by default.
            try:
                reader.explore_with_strategy(q, ExplorerStrategy.INDEX)
            except ExplorerUnsupported as e:
                assert 'ALL_VALUES' in str(e)
            else:
                raise AssertionError('expected ExplorerUnsupported for FIRST_VALUE index query')
        finally:
            reader.close()


def test_explorer_index_rejects_fts():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'index-fts.journal')
        _write_simple_entries(
            path,
            [(1_000, [('MESSAGE', b'one'), ('SERVICE', b'api')])],
        )
        reader = FileReader.open(path)
        try:
            q = (
                ExplorerQuery()
                .with_facet('SERVICE')
                .with_fts_pattern('foo')
            )
            q.field_mode = ExplorerFieldMode.ALL_VALUES
            try:
                reader.explore_with_strategy(q, ExplorerStrategy.INDEX)
            except ExplorerUnsupported as e:
                assert 'FTS' in str(e)
            else:
                raise AssertionError('expected ExplorerUnsupported for FTS index query')
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Debug-only column traversal flag is rejected (mirrors Rust L3572-3600).
# ----------------------------------------------------------------------------


def test_explorer_rejects_debug_row_traversal_column_collection():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'debug.journal')
        _write_simple_entries(path, [(1_000, [('MESSAGE', b'one')])])
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery()
            q.debug_collect_column_fields_by_row_traversal = True
            try:
                reader.explore(q)
            except ExplorerUnsupported as e:
                assert 'debug' in str(e).lower()
            else:
                raise AssertionError('expected ExplorerUnsupported for debug column traversal')
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Control: progress callback fires, cancellation stops early, deadline stops
# with TIMED_OUT, default progress interval is 250ms (mirrors Rust L3482-3535).
# ----------------------------------------------------------------------------


def test_explorer_control_progress_callback_fires_during_large_scan():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'progress.journal')
        _write_many_alternating(path, 9_000)
        reader = FileReader.open(path)
        try:
            reports = []

            def progress(p: ExplorerProgress):
                reports.append(p.stats.rows_examined)

            control = ExplorerControl()
            control.set_progress_interval(0.0)
            control.set_progress_callback(progress)
            q = ExplorerQuery()
            q.facets = [b'SERVICE']
            q.limit = 0
            result = reader.explore_with_strategy_and_control(
                q, ExplorerStrategy.TRAVERSAL, control,
            )
            assert control.stop_reason is None
            assert result.stats.rows_examined == 9_000
            assert len(reports) > 0
            # 8192-row check step means the last progress emit happens
            # at row >= 8191, so we should see it.
            assert any(r >= EXPLORER_CONTROL_CHECK_EVERY_ROWS - 1 for r in reports)
        finally:
            reader.close()


def test_explorer_control_cancellation_stops_scan_with_reason():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'cancel.journal')
        _write_many_alternating(path, 9_000)
        reader = FileReader.open(path)
        try:

            def cancel():
                return True  # cancel immediately on first check

            control = ExplorerControl()
            control.set_cancellation_callback(cancel)
            q = ExplorerQuery()
            q.facets = [b'SERVICE']
            q.limit = 0
            reader.explore_with_strategy_and_control(
                q, ExplorerStrategy.TRAVERSAL, control,
            )
            assert control.stop_reason == ExplorerStopReason.CANCELLED
        finally:
            reader.close()


def test_explorer_control_deadline_stops_scan_with_timed_out():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'deadline.journal')
        _write_many_alternating(path, 9_000)
        reader = FileReader.open(path)
        try:
            control = ExplorerControl()
            # Set a deadline that has already passed (or is about to).
            # The control check fires at most every EXPLORER_CONTROL_CHECK_EVERY_ROWS
            # rows; a near-immediate deadline is the safest cross-platform
            # way to make the scan stop with TIMED_OUT.
            control.set_deadline(time.monotonic() - 1.0)
            q = ExplorerQuery()
            q.facets = [b'SERVICE']
            q.limit = 0
            reader.explore_with_strategy_and_control(
                q, ExplorerStrategy.TRAVERSAL, control,
            )
            assert control.stop_reason == ExplorerStopReason.TIMED_OUT
        finally:
            reader.close()


def test_explorer_control_default_progress_interval_is_250ms():
    c = ExplorerControl()
    assert c.progress_interval == EXPLORER_PROGRESS_INTERVAL_MS / 1000.0
    assert c.progress_interval == 0.25


# ----------------------------------------------------------------------------
# Stats counters move as expected on a traversal.
# ----------------------------------------------------------------------------


def test_explorer_stats_sanity_on_traversal():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'stats.journal')
        _write_simple_entries(
            path,
            [
                (1_000, [('MESSAGE', b'a'), ('SERVICE', b'api')]),
                (2_000, [('MESSAGE', b'b'), ('SERVICE', b'api')]),
                (3_000, [('MESSAGE', b'c'), ('SERVICE', b'worker')]),
            ],
        )
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery().with_facet('SERVICE')
            q.use_source_realtime = False
            q.limit = 10
            result = reader.explore(q)
            assert result.stats.rows_examined == 3
            assert result.stats.rows_matched == 3
            assert result.stats.rows_returned == 3
            assert result.stats.facet_rows_matched == 3
            assert result.stats.facet_updates >= 2
            assert result.stats.last_realtime_usec == 3_000
            assert result.stats.data_refs_seen >= 6
            assert result.stats.data_objects_classified >= 3
            # All 24 counter fields exist and are integers.
            for f in (
                'rows_examined', 'rows_matched', 'facet_rows_matched',
                'rows_returned', 'rows_unsampled', 'rows_estimated',
                'sampling_sampled', 'sampling_unsampled', 'sampling_estimated',
                'last_realtime_usec', 'max_source_realtime_delta_usec',
                'data_refs_seen', 'data_refs_skipped', 'data_payloads_loaded',
                'data_objects_classified', 'data_cache_hits', 'data_cache_misses',
                'payloads_decompressed', 'fts_scans', 'facet_updates',
                'histogram_updates', 'returned_row_expansions',
                'early_stop_opportunities', 'early_stops',
            ):
                assert hasattr(result.stats, f)
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# column_fields comes from the FIELD hash-table index, not row traversal,
# and is suppressed when the debug flag is enabled (rejected outright).
# ----------------------------------------------------------------------------


def test_explorer_column_fields_come_from_field_index():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'columns.journal')
        _write_simple_entries(
            path,
            [
                (1_000, [('MESSAGE', b'a'), ('SERVICE', b'api'), ('PRIORITY', b'6')]),
                (2_000, [('MESSAGE', b'b'), ('SERVICE', b'api'), ('PRIORITY', b'5')]),
            ],
        )
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery().with_facet('SERVICE')
            result = reader.explore(q)
            expected = {n for n in ('MESSAGE', 'SERVICE', 'PRIORITY')}
            assert expected.issubset(result.column_fields)
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Field-mode: FirstValue counts one value per selected field, AllValues
# counts duplicates (mirrors Rust L4060-4104).
# ----------------------------------------------------------------------------


def test_explorer_first_value_counts_one_value_per_field():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'first.journal')
        entries = [
            (1_000, [('MESSAGE', b'one'), ('TAG', b'a'), ('TAG', b'b')]),
            (2_000, [('MESSAGE', b'two'), ('TAG', b'b')]),
        ]
        _write_simple_entries(path, entries)
        reader = FileReader.open(path)
        try:
            first = ExplorerQuery().with_facet('TAG')
            first.limit = 0
            first.use_source_realtime = False
            first_result = reader.explore(first)
            # FirstValue: each row contributes at most one TAG value
            # (the first one seen). Row 1 -> 'a'; Row 2 -> 'b'.
            assert _facet_count(first_result, b'TAG', b'a') == 1
            assert _facet_count(first_result, b'TAG', b'b') == 1

            all_values = ExplorerQuery().with_facet('TAG')
            all_values.limit = 0
            all_values.use_source_realtime = False
            all_values.field_mode = ExplorerFieldMode.ALL_VALUES
            all_result = reader.explore(all_values)
            # AllValues: row 1 contributes both 'a' and 'b'; row 2
            # contributes 'b'. So 'a' = 1, 'b' = 2.
            assert _facet_count(all_result, b'TAG', b'a') == 1
            assert _facet_count(all_result, b'TAG', b'b') == 2
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Stop-when-rows-full + stop_when_rows_full_check_every. Mirrors Rust
# should_stop_when_rows_full L3005-3036.
# ----------------------------------------------------------------------------


def test_explorer_stop_when_rows_full_truncates_scan():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'stop.journal')
        _write_simple_entries(
            path,
            [
                (1_000, [('MESSAGE', b'a'), ('SERVICE', b'api')]),
                (2_000, [('MESSAGE', b'b'), ('SERVICE', b'api')]),
                (3_000, [('MESSAGE', b'c'), ('SERVICE', b'api')]),
                (4_000, [('MESSAGE', b'd'), ('SERVICE', b'api')]),
                (5_000, [('MESSAGE', b'e'), ('SERVICE', b'api')]),
            ],
        )
        reader = FileReader.open(path)
        try:
            q = ExplorerQuery().with_filter('SERVICE', ['api'])
            q.use_source_realtime = False
            q.limit = 2
            q.stop_when_rows_full = True
            # Use a tiny slack window so the stop fires as soon as the
            # commit realtime passes the newest row plus the slack.
            q.realtime_slack_usec = 0
            result = reader.explore(q)
            # The scan should terminate before exhausting all 5 entries
            # because we already have 2 rows and the commit realtime
            # grows past newest + slack.
            assert result.stats.rows_examined < 5
            assert len(result.rows) <= 2
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# FTS terms: positive and negative patterns filter rows.
# ----------------------------------------------------------------------------


def test_explorer_fts_positive_and_negative_patterns_filter_rows():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'fts.journal')
        _write_simple_entries(
            path,
            [
                (1_000, [('MESSAGE', b'normal alpha info')]),
                (2_000, [('MESSAGE', b'normal beta info')]),
                (3_000, [('MESSAGE', b'normal gamma')]),
            ],
        )
        reader = FileReader.open(path)
        try:
            # Use the raw fts_patterns / fts_negative_patterns lists
            # (the parallel-bytes form) so positive and negative
            # patterns are evaluated independently. The
            # with_fts_pattern / with_fts_negative_pattern builders
            # mirror the Rust behavior where the fts_terms list
            # returns on the first matching term; mixing them in the
            # same query honors first-match ordering. The raw lists
            # path matches the Rust `matches_fts` helper for both
            # positive and negative axes.
            q = ExplorerQuery()
            q.fts_patterns = [b'info']
            q.fts_negative_patterns = [b'beta']
            q.use_source_realtime = False
            result = reader.explore(q)
            # Row 1 has "info" but not "beta" -> match.
            # Row 2 has "info" and "beta" -> reject.
            # Row 3 has neither -> reject.
            assert len(result.rows) == 1
            assert result.rows[0].realtime_usec == 1_000
        finally:
            reader.close()


# ----------------------------------------------------------------------------
# Query validation: invalid time window raises ExplorerError.
# ----------------------------------------------------------------------------


def test_explorer_query_validation_rejects_inverted_time_window():
    q = ExplorerQuery()
    q.after_realtime_usec = 2_000
    q.before_realtime_usec = 1_000
    try:
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, 'v.journal')
            _write_simple_entries(path, [(1_000, [('MESSAGE', b'a')])])
            reader = FileReader.open(path)
            try:
                reader.explore(q)
            finally:
                reader.close()
    except ExplorerError as e:
        assert 'after_realtime_usec' in str(e)
    else:
        raise AssertionError('expected ExplorerError for inverted time window')


def test_explorer_query_validation_rejects_duplicate_facets():
    q = ExplorerQuery().with_facet('SERVICE').with_facet('SERVICE')
    try:
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, 'dup.journal')
            _write_simple_entries(path, [(1_000, [('SERVICE', b'api')])])
            reader = FileReader.open(path)
            try:
                reader.explore(q)
            finally:
                reader.close()
    except ExplorerError as e:
        assert 'duplicate' in str(e).lower()
    else:
        raise AssertionError('expected ExplorerError for duplicate facets')


# ----------------------------------------------------------------------------
# When running this file directly, run all tests.
# ----------------------------------------------------------------------------


def _collect():
    return [v for k, v in sorted(globals().items()) if k.startswith('test_') and callable(v)]


def main():
    tests = _collect()
    failed = 0
    for t in tests:
        try:
            t()
        except AssertionError as e:
            failed += 1
            print('FAIL {0}: {1}'.format(t.__name__, e))
        except Exception as e:
            failed += 1
            print('ERROR {0}: {1}: {2}'.format(t.__name__, type(e).__name__, e))
        else:
            print('PASS {0}'.format(t.__name__))
    if failed:
        print('FAILED {0} of {1}'.format(failed, len(tests)))
        sys.exit(1)
    print('PASS explorer tests ({0})'.format(len(tests)))


if __name__ == '__main__':
    main()
