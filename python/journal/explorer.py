# Pure-Python journal explorer.
#
# Mirrors the semantics of rust/src/journal/src/explorer.rs (the source of
# truth) for the public surface listed in SOW-0104. Go deviations from
# go/journal/explorer.go are intentionally not copied here: Rust uses
# tagged unions like `ExplorerAnchor::Realtime(u64)`, while Go models
# them as a struct with a kind + payload. Python mirrors Rust semantics
# with a small dataclass carrying the kind enum plus an optional
# `realtime_usec` value, and uses `Optional[ExplorerStopReason]` for
# "no stop" rather than Go's `ExplorerStopNone` sentinel.

import time
from dataclasses import dataclass, field, replace
from enum import Enum
from typing import Callable, Dict, List, Optional, Set

from .header import HASH_ITEM_SIZE


# Constants mirroring rust/src/journal/src/explorer.rs:6-16. Keep the
# numeric values in lockstep with Rust and Go so the cross-language
# suites compare equal.
DEFAULT_HISTOGRAM_TARGET_BUCKETS = 150
DEFAULT_TIME_SLACK_USEC = 120_000_000
EXPLORER_CONTROL_CHECK_EVERY_ROWS = 8192
DEFAULT_ROWS_FULL_CHECK_EVERY_ROWS = 1
EXPLORER_PROGRESS_INTERVAL_MS = 250
EXPLORER_SAMPLING_SLOTS_MAX = 1000
EXPLORER_SAMPLING_RECALIBRATE_ROWS = 10_000
EXPLORER_SAMPLING_ESTIMATE_AFTER_PROGRESS = 0.01
EXPLORER_HISTOGRAM_MAX_BUCKETS = 1001
EXPLORER_HISTOGRAM_DEFAULT_WINDOW_USEC = 3_600_000_000

SOURCE_REALTIME_FIELD = b'_SOURCE_REALTIME_TIMESTAMP'
UNSET_VALUE = b'-'
EXPLORER_UNSAMPLED_VALUE = b'[unsampled]'
EXPLORER_ESTIMATED_VALUE = b'[estimated]'


# Field-flag bits for the accumulator; mirror the Rust constants.
_FACET_PUBLIC = 0x01
_FACET_HISTOGRAM = 0x02
_FACET_SOURCE_REALTIME = 0x04


class ExplorerError(ValueError):
    """Explorer query validation or compatibility error."""


class ExplorerUnsupported(ExplorerError):
    """Operation the explorer cannot perform in its current mode."""


class Direction(Enum):
    """Direction of iteration. Forward = head to tail, Backward = tail to head."""

    FORWARD = 0
    BACKWARD = 1


class ExplorerAnchorKind(Enum):
    """Anchor kind for `ExplorerAnchor`. Mirrors Rust `ExplorerAnchor` (L19-24)."""

    AUTO = 'auto'
    HEAD = 'head'
    TAIL = 'tail'
    REALTIME = 'realtime'


@dataclass(frozen=True)
class ExplorerAnchor:
    """Anchor for traversal start position.

    Rust uses `ExplorerAnchor::Realtime(u64)` (L19-24); Go uses a
    struct with a kind enum + realtime_usec. Python mirrors the Rust
    semantics with this dataclass: when `kind` is `REALTIME`, use
    `realtime_usec`; otherwise `realtime_usec` is ignored. `AUTO` is
    the default and means "head when scanning forward, tail when
    scanning backward".
    """

    kind: ExplorerAnchorKind = ExplorerAnchorKind.AUTO
    realtime_usec: int = 0

    @staticmethod
    def auto():
        return ExplorerAnchor()

    @staticmethod
    def head():
        return ExplorerAnchor(kind=ExplorerAnchorKind.HEAD)

    @staticmethod
    def tail():
        return ExplorerAnchor(kind=ExplorerAnchorKind.TAIL)

    @staticmethod
    def realtime(usec):
        return ExplorerAnchor(kind=ExplorerAnchorKind.REALTIME, realtime_usec=int(usec))


class ExplorerFieldMode(Enum):
    """Field mode: how many values per field to count."""

    ALL_VALUES = 'all_values'
    FIRST_VALUE = 'first_value'


class ExplorerStrategy(Enum):
    """Execution strategy for an explorer query."""

    TRAVERSAL = 'traversal'
    INDEX = 'index'
    COMPARE = 'compare'


class ExplorerStopReason(Enum):
    """Reason the explorer stopped early. Mirrors Rust (L290-295)."""

    TIMED_OUT = 'timed_out'
    CANCELLED = 'cancelled'


@dataclass
class ExplorerFilter:
    """Explorer filter on a field with one or more values (OR semantics)."""

    field: bytes
    values: List[bytes] = field(default_factory=list)

    @staticmethod
    def new(field, values):
        return ExplorerFilter(field=_to_bytes(field), values=[_to_bytes(v) for v in values])


@dataclass
class ExplorerFtsPattern:
    """Explorer FTS substring pattern.

    Rust semantics (L145-179): the pattern is split on `*`, empty
    parts dropped, then each part must match the haystack in order
    as an ASCII-case-insensitive substring with the haystack
    advancing past each match. Empty value never matches.
    """

    parts: List[bytes] = field(default_factory=list)
    negative: bool = False

    @staticmethod
    def substring(pattern, negative=False):
        pattern_bytes = _to_bytes(pattern)
        parts = [bytes(p) for p in pattern_bytes.split(b'*') if len(p) > 0]
        return ExplorerFtsPattern(parts=parts, negative=bool(negative))

    def matches(self, value):
        if isinstance(value, str):
            value = value.encode('utf-8')
        if not value:
            return False
        if not self.parts:
            return True
        haystack = value
        for part in self.parts:
            index = _find_ascii_case_insensitive(haystack, part)
            if index is None:
                return False
            haystack = haystack[index + len(part):]
        return True


@dataclass
class ExplorerSampling:
    """Sampling hint for explorer queries.

    Rust: L134-143. Mirrors Go (ExplorerSampling, L154-162). All
    fields are u64 with the same names; sentinel zero values disable
    the corresponding optimization.
    """

    budget: int = 0
    matched_files: int = 0
    file_head_realtime_usec: int = 0
    file_tail_realtime_usec: int = 0
    file_head_seqnum: int = 0
    file_tail_seqnum: int = 0
    file_entries: int = 0


@dataclass
class ExplorerStats:
    """All 24 counter fields from Rust (L218-244). Field names match
    the Rust/Go JSON serialization names exactly.
    """

    rows_examined: int = 0
    rows_matched: int = 0
    facet_rows_matched: int = 0
    rows_returned: int = 0
    rows_unsampled: int = 0
    rows_estimated: int = 0
    sampling_sampled: int = 0
    sampling_unsampled: int = 0
    sampling_estimated: int = 0
    last_realtime_usec: int = 0
    max_source_realtime_delta_usec: int = 0
    data_refs_seen: int = 0
    data_refs_skipped: int = 0
    data_payloads_loaded: int = 0
    data_objects_classified: int = 0
    data_cache_hits: int = 0
    data_cache_misses: int = 0
    payloads_decompressed: int = 0
    fts_scans: int = 0
    facet_updates: int = 0
    histogram_updates: int = 0
    returned_row_expansions: int = 0
    early_stop_opportunities: int = 0
    early_stops: int = 0

    def copy(self):
        return replace(self)


@dataclass
class ExplorerRow:
    """Explorer matched row. Mirrors Rust (L246-251)."""

    realtime_usec: int
    cursor: str
    payloads: List[bytes] = field(default_factory=list)


@dataclass
class ExplorerHistogramBucket:
    """Histogram bucket. Rust (L260-264)."""

    start_realtime_usec: int
    end_realtime_usec: int
    values: Dict[bytes, int] = field(default_factory=dict)


@dataclass
class ExplorerHistogram:
    """Histogram over a single field. Rust (L266-270)."""

    field: bytes
    buckets: List[ExplorerHistogramBucket] = field(default_factory=list)


@dataclass
class ExplorerComparison:
    """Compare-strategy diagnostics. Rust (L272-278)."""

    traversal_duration: float = 0.0
    index_duration: float = 0.0
    traversal_stats: ExplorerStats = field(default_factory=ExplorerStats)
    index_stats: ExplorerStats = field(default_factory=ExplorerStats)


@dataclass
class ExplorerResult:
    """Explorer query result. Rust (L280-288)."""

    rows: List[ExplorerRow] = field(default_factory=list)
    facets: Dict[bytes, Dict[bytes, int]] = field(default_factory=dict)
    histogram: Optional[ExplorerHistogram] = None
    column_fields: Set[bytes] = field(default_factory=set)
    stats: ExplorerStats = field(default_factory=ExplorerStats)
    comparison: Optional[ExplorerComparison] = None


@dataclass
class ExplorerProgress:
    """Explorer progress callback payload. Rust (L296-300)."""

    stats: ExplorerStats = field(default_factory=ExplorerStats)
    elapsed: float = 0.0


@dataclass
class ExplorerQuery:
    """Full explorer query. Mirrors Rust (L76-132) with 22 public fields
    and all documented defaults.

    Python adaptation: Rust's `mut self` consuming-builder is
    modeled with in-place mutation returning `self` (the dataclass
    is mutable). The chained style (`ExplorerQuery().with_filter(...)
    .with_facet(...)`) works the same way as the Rust API.
    """

    after_realtime_usec: Optional[int] = None
    before_realtime_usec: Optional[int] = None
    anchor: ExplorerAnchor = field(default_factory=ExplorerAnchor.auto)
    direction: Direction = Direction.FORWARD
    limit: int = 200
    filters: List[ExplorerFilter] = field(default_factory=list)
    facets: List[bytes] = field(default_factory=list)
    histogram: Optional[bytes] = None
    histogram_after_realtime_usec: Optional[int] = None
    histogram_before_realtime_usec: Optional[int] = None
    histogram_target_buckets: int = DEFAULT_HISTOGRAM_TARGET_BUCKETS
    fts_terms: List[ExplorerFtsPattern] = field(default_factory=list)
    fts_patterns: List[bytes] = field(default_factory=list)
    fts_negative_patterns: List[bytes] = field(default_factory=list)
    field_mode: ExplorerFieldMode = ExplorerFieldMode.FIRST_VALUE
    exclude_facet_field_filters: bool = True
    use_source_realtime: bool = True
    realtime_slack_usec: int = DEFAULT_TIME_SLACK_USEC
    stop_when_rows_full: bool = False
    stop_when_rows_full_check_every: int = DEFAULT_ROWS_FULL_CHECK_EVERY_ROWS
    sampling: Optional[ExplorerSampling] = None
    # Debug-only discrepancy tool. Production callers must never
    # set this; column catalogs belong to the FIELD hash-table
    # path, not row traversal. The runner rejects this with an
    # `ExplorerUnsupported` error.
    debug_collect_column_fields_by_row_traversal: bool = False

    def with_filter(self, field, values):
        self.filters.append(ExplorerFilter.new(field, values))
        return self

    def with_facet(self, field):
        self.facets.append(_to_bytes(field))
        return self

    def with_histogram(self, field):
        self.histogram = _to_bytes(field)
        return self

    def with_fts_pattern(self, pattern):
        pattern_bytes = _to_bytes(pattern)
        self.fts_terms.append(ExplorerFtsPattern.substring(pattern_bytes, negative=False))
        self.fts_patterns.append(pattern_bytes)
        return self

    def with_fts_negative_pattern(self, pattern):
        pattern_bytes = _to_bytes(pattern)
        self.fts_terms.append(ExplorerFtsPattern.substring(pattern_bytes, negative=True))
        self.fts_negative_patterns.append(pattern_bytes)
        return self


# ExplorerControl callback signatures. Documented in the SOW as Rust's
# internal `&dyn FnMut` shapes. Python uses Optional callables and
# lets the control object own all state.
ProgressCallback = Callable[[ExplorerProgress], None]
CancellationCallback = Callable[[], bool]
MatchedRowCallback = Callable[[int, int], bool]


@dataclass
class ExplorerControl:
    """Explorer execution control. Mirrors Rust (L302-379).

    Callbacks are optional. `deadline` is an absolute monotonic time
    in seconds (like `time.monotonic()`). `stop_reason` becomes
    non-None when a deadline or cancellation stopped the scan.
    """

    deadline: Optional[float] = None
    cancellation: Optional[CancellationCallback] = None
    progress: Optional[ProgressCallback] = None
    matched_row: Optional[MatchedRowCallback] = None
    progress_interval: float = EXPLORER_PROGRESS_INTERVAL_MS / 1000.0
    stop_reason: Optional[ExplorerStopReason] = None
    _started: float = field(default_factory=time.monotonic, init=False, repr=False)
    _last_progress: float = field(default_factory=time.monotonic, init=False, repr=False)
    _next_check_rows: int = EXPLORER_CONTROL_CHECK_EVERY_ROWS
    _stopped: bool = field(default=False, init=False, repr=False)

    def set_deadline(self, deadline):
        self.deadline = deadline

    def set_cancellation_callback(self, callback):
        self.cancellation = callback

    def set_progress_callback(self, callback):
        self.progress = callback

    def set_matched_row_callback(self, callback):
        self.matched_row = callback

    def set_progress_interval(self, interval):
        self.progress_interval = float(interval)

    def should_stop_after_rows(self, rows_seen, stats):
        if self._stopped:
            return True
        if rows_seen < self._next_check_rows:
            return False
        self._next_check_rows = rows_seen + EXPLORER_CONTROL_CHECK_EVERY_ROWS
        return self._check(stats)

    def _check(self, stats):
        now = time.monotonic()
        if self.progress is not None and (now - self._last_progress) >= self.progress_interval:
            self._emit_progress(stats, now)
        if self.cancellation is not None and self.cancellation():
            self.stop_reason = ExplorerStopReason.CANCELLED
            self._emit_progress(stats, now)
            self._stopped = True
            return True
        if self.deadline is not None and now >= self.deadline:
            self.stop_reason = ExplorerStopReason.TIMED_OUT
            self._emit_progress(stats, now)
            self._stopped = True
            return True
        return False

    def _emit_progress(self, stats, now):
        self._last_progress = now
        if self.progress is not None:
            self.progress(ExplorerProgress(stats=stats.copy(), elapsed=now - self._started))

    def emit_matched_row(self, realtime_usec, rows_matched):
        if self.matched_row is None:
            return False
        return bool(self.matched_row(int(realtime_usec), int(rows_matched)))


# ----------------------------------------------------------------------------
# Internal helpers: byte/str normalization, ASCII case-insensitive search.
# ----------------------------------------------------------------------------


def _to_bytes(value):
    if isinstance(value, bytes):
        return value
    if isinstance(value, bytearray):
        return bytes(value)
    if isinstance(value, memoryview):
        return bytes(value)
    if isinstance(value, str):
        return value.encode('utf-8')
    raise TypeError(f'expected bytes/str, got {type(value).__name__}')


def _split_payload(payload):
    if isinstance(payload, memoryview):
        payload = bytes(payload)
    eq = payload.find(b'=')
    if eq < 0:
        return None
    return (bytes(payload[:eq]), bytes(payload[eq + 1:]))


def _parse_source_realtime(value):
    if not value:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _find_ascii_case_insensitive(haystack, needle):
    if not needle:
        return 0
    if len(haystack) < len(needle):
        return None
    n = len(needle)
    last = len(haystack) - n
    for i in range(last + 1):
        if _ascii_equal_fold(haystack[i:i + n], needle):
            return i
    return None


def _contains_ascii_case_insensitive(haystack, needle):
    return _find_ascii_case_insensitive(haystack, needle) is not None


def _ascii_equal_fold(a, b):
    if len(a) != len(b):
        return False
    for x, y in zip(a, b):
        if not _ascii_eq(x, y):
            return False
    return True


def _ascii_eq(a, b):
    if 0x41 <= a <= 0x5A:
        a += 0x20
    if 0x41 <= b <= 0x5A:
        b += 0x20
    return a == b


# ----------------------------------------------------------------------------
# Validation helpers (mirrors Rust validate_query and validate_indexed_query).
# ----------------------------------------------------------------------------


def _validate_query(query):
    if (
        query.after_realtime_usec is not None
        and query.before_realtime_usec is not None
        and query.after_realtime_usec > query.before_realtime_usec
    ):
        raise ExplorerError('after_realtime_usec must be <= before_realtime_usec')
    for f in query.filters:
        if not f.field or b'=' in f.field:
            raise ExplorerError('filter field must be non-empty and must not contain \'=\'')
    for f in query.facets:
        if not f or b'=' in f:
            raise ExplorerError('facet and histogram fields must be non-empty and must not contain \'=\'')
    if query.histogram is not None:
        if not query.histogram or b'=' in query.histogram:
            raise ExplorerError('facet and histogram fields must be non-empty and must not contain \'=\'')
    seen = set()
    for f in query.facets:
        if f in seen:
            raise ExplorerError('facet fields must not be duplicated')
        seen.add(f)


def _validate_no_debug_column_collection(query):
    if query.debug_collect_column_fields_by_row_traversal:
        raise ExplorerUnsupported(
            'debug_collect_column_fields_by_row_traversal is a debug-only discrepancy tool; '
            'production explorer queries must use FIELD-index column catalogs instead'
        )


def _validate_indexed_query(query):
    if query.field_mode != ExplorerFieldMode.ALL_VALUES:
        raise ExplorerUnsupported(
            'indexed explorer strategy requires ExplorerFieldMode.ALL_VALUES'
        )
    if _query_has_fts(query):
        raise ExplorerUnsupported('indexed explorer strategy does not support FTS')
    if query.use_source_realtime and (
        query.after_realtime_usec is not None
        or query.before_realtime_usec is not None
        or query.histogram is not None
    ):
        raise ExplorerUnsupported(
            'indexed explorer strategy requires commit realtime for time-bounded facets and histograms'
        )


def _query_has_fts(query):
    return bool(query.fts_terms) or bool(query.fts_patterns) or bool(query.fts_negative_patterns)


def _query_has_positive_fts(query):
    if query.fts_terms:
        return any(not t.negative for t in query.fts_terms)
    return bool(query.fts_patterns)


def _row_rejected_by_fts(query, fts_matches, fts_negative):
    if not _query_has_fts(query):
        return False
    if fts_negative:
        return True
    if _query_has_positive_fts(query) and not fts_matches:
        return True
    return False


# ----------------------------------------------------------------------------
# Histogram bucketing (mirrors Rust new_histogram, histogram_bounds,
# histogram_bar_width_usec, histogram_slot_baseline_usec, and the 1001 cap).
# ----------------------------------------------------------------------------


_VALID_HISTOGRAM_BAR_SECONDS = (
    1, 2, 5, 10, 15, 30, 60, 120, 180, 300, 600, 900,
    1800, 3600, 7200, 21600, 28800, 43200, 86400,
    172800, 259200, 432000, 604800, 1209600, 2592000,
)


def _histogram_bounds(query):
    start = (
        query.histogram_after_realtime_usec
        if query.histogram_after_realtime_usec is not None
        else (query.after_realtime_usec if query.after_realtime_usec is not None else 0)
    )
    if query.histogram_before_realtime_usec is not None:
        end = query.histogram_before_realtime_usec
    elif query.before_realtime_usec is not None:
        end = query.before_realtime_usec
    else:
        end = start + EXPLORER_HISTOGRAM_DEFAULT_WINDOW_USEC
    if end <= start:
        return start, start + 1
    return start, end


def _histogram_bar_width_usec(after, before, target_buckets):
    usec_per_sec = 1_000_000
    duration = before - after
    for seconds in reversed(_VALID_HISTOGRAM_BAR_SECONDS):
        width = seconds * usec_per_sec
        if width != 0 and duration // width >= target_buckets:
            return width
    return usec_per_sec


def _histogram_slot_baseline_usec(value, width):
    if width <= 0:
        width = 1
    return value - (value % width)


def _new_histogram(field, query):
    start, end = _histogram_bounds(query)
    target_buckets = max(1, int(query.histogram_target_buckets))
    width = _histogram_bar_width_usec(start, end, target_buckets)
    start = _histogram_slot_baseline_usec(start, width)
    end = _histogram_slot_baseline_usec(end, width) + width
    bucket_count = ((end - start) // width) + 1
    if bucket_count > EXPLORER_HISTOGRAM_MAX_BUCKETS:
        bucket_count = EXPLORER_HISTOGRAM_MAX_BUCKETS
        width = max(1, (end - start) // 1000)
        end = start + width * 1000
    buckets = []
    for i in range(bucket_count):
        bucket_start = start + width * i
        bucket_end = end + 1 if i + 1 == bucket_count else bucket_start + width
        buckets.append(
            ExplorerHistogramBucket(
                start_realtime_usec=int(bucket_start),
                end_realtime_usec=int(bucket_end),
            )
        )
    return ExplorerHistogram(field=field, buckets=buckets)


def _histogram_bucket_index(histogram, realtime_usec):
    if not histogram.buckets:
        return None
    first = histogram.buckets[0]
    width = first.end_realtime_usec - first.start_realtime_usec
    if width <= 0:
        width = 1
    return _histogram_bucket_index_from_bounds(
        realtime_usec, first.start_realtime_usec, width, len(histogram.buckets)
    )


def _histogram_bucket_index_from_bounds(realtime_usec, start, width, count):
    if count == 0:
        return None
    if width <= 0:
        width = 1
    if realtime_usec < start:
        return 0
    index = (realtime_usec - start) // width
    if index >= count:
        index = count - 1
    return int(index)


# ----------------------------------------------------------------------------
# Time-window helpers (mirrors Rust timestamp_in_range, stop_by_commit_time,
# skip_by_commit_time, and the row_within_anchor / should_stop_when_rows_full
# / row_candidate_to_keep family).
# ----------------------------------------------------------------------------


def _timestamp_in_range(query, ts):
    if query.after_realtime_usec is not None and ts < query.after_realtime_usec:
        return False
    if query.before_realtime_usec is not None and ts > query.before_realtime_usec:
        return False
    return True


def _stop_by_commit_time(query, commit_realtime):
    if query.direction == Direction.FORWARD:
        if query.before_realtime_usec is None:
            return False
        return commit_realtime > query.before_realtime_usec + query.realtime_slack_usec
    if query.after_realtime_usec is None:
        return False
    return commit_realtime < query.after_realtime_usec


def _skip_by_commit_time(query, commit_realtime):
    if query.direction == Direction.FORWARD:
        if query.after_realtime_usec is None:
            return False
        return commit_realtime < query.after_realtime_usec
    if query.before_realtime_usec is None:
        return False
    return commit_realtime > query.before_realtime_usec + query.realtime_slack_usec


def _row_within_anchor(query, realtime_usec):
    if query.anchor.kind != ExplorerAnchorKind.REALTIME:
        return True
    if query.direction == Direction.FORWARD:
        return realtime_usec > query.anchor.realtime_usec
    return realtime_usec <= query.anchor.realtime_usec


def _row_candidate_to_keep(query, rows, realtime_usec):
    if query.limit == 0:
        return False
    if not _row_within_anchor(query, realtime_usec):
        return False
    if len(rows) < query.limit:
        return True
    if query.direction == Direction.BACKWARD:
        oldest = min(r.realtime_usec for r in rows)
        return realtime_usec >= oldest
    newest = max(r.realtime_usec for r in rows)
    return realtime_usec <= newest


def _should_stop_when_rows_full(query, rows, effective_realtime, rows_matched):
    if not query.stop_when_rows_full or query.limit == 0 or len(rows) < query.limit:
        return False
    every = max(1, int(query.stop_when_rows_full_check_every))
    if rows_matched == 0 or rows_matched % every != 0:
        return False
    if query.direction == Direction.BACKWARD:
        oldest = min(r.realtime_usec for r in rows)
        return effective_realtime < oldest - query.realtime_slack_usec
    newest = max(r.realtime_usec for r in rows)
    return effective_realtime > newest + query.realtime_slack_usec


def _effective_realtime_from_scan(source_realtime, commit_realtime):
    if source_realtime is not None and source_realtime != 0 and source_realtime < commit_realtime:
        return int(source_realtime)
    return int(commit_realtime)


def _record_last_realtime(stats, commit_realtime):
    if commit_realtime > stats.last_realtime_usec:
        stats.last_realtime_usec = int(commit_realtime)


def _record_source_realtime_delta(stats, source_realtime, commit_realtime):
    if source_realtime is None or source_realtime == 0 or source_realtime >= commit_realtime:
        return
    delta = commit_realtime - source_realtime
    if delta > stats.max_source_realtime_delta_usec:
        stats.max_source_realtime_delta_usec = int(delta)


def _query_needs_main_pass(query):
    return query.limit > 0 or query.histogram is not None


def _query_needs_source_realtime_main(query):
    return query.use_source_realtime and (
        query.after_realtime_usec is not None
        or query.before_realtime_usec is not None
        or query.histogram is not None
        or query.limit > 0
    )


def _facet_pass_needs_source_realtime(query):
    return query.use_source_realtime and (
        query.after_realtime_usec is not None or query.before_realtime_usec is not None
    )


# ----------------------------------------------------------------------------
# FTS matching for a single value.
# ----------------------------------------------------------------------------


def _match_fts_query(value, query):
    if query.fts_terms:
        for term in query.fts_terms:
            if term.matches(value):
                return (False, True) if term.negative else (True, False)
        return (False, False)
    if any(_contains_ascii_case_insensitive(value, pat) for pat in query.fts_negative_patterns if pat):
        return (False, True)
    if any(_contains_ascii_case_insensitive(value, pat) for pat in query.fts_patterns if pat):
        return (True, False)
    return (False, False)


# ----------------------------------------------------------------------------
# Offset-class classification cache (mirrors Rust OffsetClassCache).
# ----------------------------------------------------------------------------


_OFFSET_CLASS_IRRELEVANT = 1
_OFFSET_CLASS_FTS_MATCH = 2
_OFFSET_CLASS_FTS_NEGATIVE = 3
_OFFSET_CLASS_VALUE_BASE = 4


class _OffsetClassCache:
    """Per-row classification cache for DATA offsets.

    Mirrors Rust `OffsetClassCache` (L838-903). The hot path is
    "did we already classify this offset?"; production passes reuse
    DATA objects many times because dedup'd entries share them.
    Python dict is the closest analogue to Rust's open-addressed
    hash table and is what the cache uses.
    """

    __slots__ = ('_slots', '_len')

    def __init__(self):
        self._slots = {}
        self._len = 0

    def lookup(self, offset):
        return self._slots.get(offset)

    def insert(self, offset, class_value):
        if offset == 0:
            return
        self._slots[offset] = class_value
        self._len += 1


# ----------------------------------------------------------------------------
# Facet pass group and the can-run-combined helpers.
# ----------------------------------------------------------------------------


@dataclass
class _FacetPassGroup:
    excluded_field: Optional[bytes]
    facet_indices: List[int]


def _facet_pass_groups(query):
    filter_fields = {f.field for f in query.filters}
    groups = []
    for idx, facet in enumerate(query.facets):
        excluded = None
        if query.exclude_facet_field_filters and facet in filter_fields:
            excluded = facet
        for group in groups:
            if group.excluded_field == excluded:
                group.facet_indices.append(idx)
                break
        else:
            groups.append(_FacetPassGroup(excluded_field=excluded, facet_indices=[idx]))
    return groups


def _can_run_combined_pass(groups):
    return all(g.excluded_field is None for g in groups)


def _combined_facet_indices(groups):
    out = []
    for g in groups:
        out.extend(g.facet_indices)
    return out


# ----------------------------------------------------------------------------
# Accumulator (mirrors Rust ExplorerAccumulator).
# ----------------------------------------------------------------------------


class _ExplorerAccumulator:
    __slots__ = (
        'field_lookup',
        'fields',
        'flags',
        'last_seen_row_ids',
        'unset_counts',
        'values_by_field',
        'value_counts',
        'value_field_indices',
        'value_labels',
        'value_fts_matches',
        'value_source_realtime',
        'value_histogram_buckets',
        'field_histogram_unset_buckets',
        'offset_cache',
        'histogram_start_realtime_usec',
        'histogram_bucket_width_usec',
        'histogram_bucket_count',
        'required_identity_count',
    )

    def __init__(self, histogram):
        self.field_lookup = {}
        self.fields = []
        self.flags = []
        self.last_seen_row_ids = []
        self.unset_counts = []
        self.values_by_field = []
        self.value_counts = []
        self.value_field_indices = []
        self.value_labels = []
        self.value_fts_matches = []
        self.value_source_realtime = []
        self.value_histogram_buckets = []
        self.field_histogram_unset_buckets = []
        self.offset_cache = _OffsetClassCache()
        if histogram is not None and histogram.buckets:
            first = histogram.buckets[0]
            self.histogram_start_realtime_usec = first.start_realtime_usec
            width = first.end_realtime_usec - first.start_realtime_usec
            if width <= 0:
                width = 1
            self.histogram_bucket_width_usec = width
            self.histogram_bucket_count = len(histogram.buckets)
        else:
            self.histogram_start_realtime_usec = 0
            self.histogram_bucket_width_usec = 1
            self.histogram_bucket_count = 0
        self.required_identity_count = 0

    def add_field(self, field, flags):
        idx = self.field_lookup.get(field)
        if idx is not None:
            had_required = self.flags[idx] != 0
            self.flags[idx] |= flags
            if (flags & _FACET_HISTOGRAM) != 0 and self.field_histogram_unset_buckets[idx] is None:
                self.field_histogram_unset_buckets[idx] = [0] * self.histogram_bucket_count
            if not had_required and self.flags[idx] != 0:
                self.required_identity_count += 1
            return idx
        idx = len(self.fields)
        self.field_lookup[field] = idx
        self.fields.append(field)
        self.flags.append(flags)
        self.last_seen_row_ids.append(0)
        self.unset_counts.append(0)
        self.values_by_field.append([])
        if (flags & _FACET_HISTOGRAM) != 0:
            self.field_histogram_unset_buckets.append([0] * self.histogram_bucket_count)
        else:
            self.field_histogram_unset_buckets.append(None)
        if flags != 0:
            self.required_identity_count += 1
        return idx

    def add_value(self, field_idx, value, fts_matches):
        value_index = len(self.value_counts)
        flags = self.flags[field_idx]
        self.value_counts.append(0)
        self.value_field_indices.append(field_idx)
        self.value_labels.append(value)
        self.value_fts_matches.append(fts_matches)
        if (flags & _FACET_SOURCE_REALTIME) != 0:
            self.value_source_realtime.append(_parse_source_realtime(value))
        else:
            self.value_source_realtime.append(None)
        if (flags & _FACET_HISTOGRAM) != 0:
            self.value_histogram_buckets.append([0] * self.histogram_bucket_count)
        else:
            self.value_histogram_buckets.append(None)
        self.values_by_field[field_idx].append(value_index)
        return value_index

    def mark_field_seen(self, field_idx, row_id):
        if self.last_seen_row_ids[field_idx] == row_id:
            return False
        self.last_seen_row_ids[field_idx] = row_id
        return True

    def apply_value(self, value_index, realtime_usec, stats):
        field_idx = self.value_field_indices[value_index]
        flags = self.flags[field_idx]
        if (flags & _FACET_PUBLIC) != 0:
            self.value_counts[value_index] += 1
            stats.facet_updates += 1
        if (flags & _FACET_HISTOGRAM) != 0 and realtime_usec is not None:
            buckets = self.value_histogram_buckets[value_index]
            if buckets is not None:
                idx = _histogram_bucket_index_from_bounds(
                    realtime_usec,
                    self.histogram_start_realtime_usec,
                    self.histogram_bucket_width_usec,
                    len(buckets),
                )
                if idx is not None:
                    buckets[idx] += 1
                    stats.histogram_updates += 1

    def finish_facet_row(self, row_id, stats):
        for i, flag in enumerate(self.flags):
            if (flag & _FACET_PUBLIC) == 0:
                continue
            if self.last_seen_row_ids[i] != row_id:
                self.unset_counts[i] += 1
                stats.facet_updates += 1

    def finish_histogram_row(self, row_id, realtime_usec, stats):
        for i, flag in enumerate(self.flags):
            if (flag & _FACET_HISTOGRAM) == 0:
                continue
            if self.last_seen_row_ids[i] == row_id:
                continue
            buckets = self.field_histogram_unset_buckets[i]
            if buckets is None:
                continue
            idx = _histogram_bucket_index_from_bounds(
                realtime_usec,
                self.histogram_start_realtime_usec,
                self.histogram_bucket_width_usec,
                len(buckets),
            )
            if idx is not None:
                buckets[idx] += 1
                stats.histogram_updates += 1

    def finish_facets(self, result):
        for i, field in enumerate(self.fields):
            if (self.flags[i] & _FACET_PUBLIC) == 0:
                continue
            values = {}
            for value_index in self.values_by_field[i]:
                count = self.value_counts[value_index]
                if count:
                    _increment_counter(values, self.value_labels[value_index], count)
            if self.unset_counts[i]:
                _increment_counter(values, UNSET_VALUE, self.unset_counts[i])
            result.facets[field] = values

    def finish_histogram(self, histogram):
        if histogram is None:
            return
        for buckets in self.field_histogram_unset_buckets:
            if buckets is None:
                continue
            for bucket_idx, count in enumerate(buckets):
                if count == 0:
                    continue
                bucket = histogram.buckets[bucket_idx]
                _increment_counter(bucket.values, UNSET_VALUE, count)
        for value_index, buckets in enumerate(self.value_histogram_buckets):
            if buckets is None:
                continue
            for bucket_idx, count in enumerate(buckets):
                if count == 0:
                    continue
                bucket = histogram.buckets[bucket_idx]
                _increment_counter(bucket.values, self.value_labels[value_index], count)


def _increment_counter(counter, key, delta):
    if key in counter:
        counter[key] += delta
    else:
        counter[key] = delta


# ----------------------------------------------------------------------------
# Field-enumeration (FIELD-index catalog path).
# ----------------------------------------------------------------------------


def _enumerate_fields_indexed(reader):
    """FIELD hash-table path for column catalogs. Mirrors Rust's
    ExplorerResult.column_fields population via field indexes.
    """

    header = reader.header()
    table_offset = header.get('field_hash_table_offset', 0) or 0
    table_size = header.get('field_hash_table_size', 0) or 0
    fields = set()
    if table_offset == 0 or table_size < HASH_ITEM_SIZE:
        return fields
    buckets = table_size // HASH_ITEM_SIZE
    for bucket in range(buckets):
        bucket_offset = table_offset + bucket * HASH_ITEM_SIZE
        if len(reader._buffer) < bucket_offset + HASH_ITEM_SIZE:
            raise ValueError('field hash bucket exceeds buffer')
        offset = reader._UNPACK_U64(bucket_offset)
        while offset:
            field_obj = reader._read_field_object_at(offset)
            try:
                field_name = field_obj['payload'].decode('utf-8')
            except UnicodeDecodeError:
                field_name = None
            if field_name is not None:
                fields.add(field_name)
            offset = field_obj['next_hash_offset']
    return fields


# ----------------------------------------------------------------------------
# Reader shim helpers: a few operations the explorer needs are private
# to FileReader today. Rather than touching the public reader API, we
# add internal `_explorer_*` methods on FileReader and DirectoryReader
# in their own files; this module calls them through duck-typing.
# ----------------------------------------------------------------------------


def _flush_reader_filters(reader):
    reader.flush_matches()


def _reader_filter_matches(reader):
    """Check if the reader has an active filter and whether the current
    entry matches it.

    Rust applies filters at the index level during ``next()`` /
    ``previous()`` using the journal's DATA hash table.  The Python
    reader does not implement index-based filtering, so we perform a
    manual entry-level check after stepping to a new entry.

    Returns True when no filter is set or the current entry matches.
    """
    if reader._filter is None:
        return True
    try:
        entry = reader._read_entry_at(
            reader._entry_offsets[reader._entry_index]
        )
    except Exception:
        return True
    return reader._filter.matches(entry)


def _configure_filters(reader, query, excluded_field):
    """Push active filters into the reader for index-based row skipping.

    Mirrors Rust `configure_explorer_filters` (L1528-1547): the
    reader's filter builder is reset and the union of all filter
    values per field is added. Filters with empty values are
    skipped; an excluded field is also skipped (used by the
    per-facet-group path that wants the original distribution
    inside the facet field).
    """

    _flush_reader_filters(reader)
    for f in query.filters:
        if excluded_field is not None and f.field == excluded_field:
            continue
        if not f.values:
            continue
        for v in f.values:
            reader.add_match(f.field + b'=' + v)


def _seek_for_explorer(reader, query):
    """Mirror Rust seek_for_explorer (L2096-2140)."""

    anchor = query.anchor if query.stop_when_rows_full else ExplorerAnchor.auto()
    if query.direction == Direction.FORWARD:
        if anchor.kind == ExplorerAnchorKind.REALTIME:
            reader.seek_realtime_usec(anchor.realtime_usec)
        elif anchor.kind == ExplorerAnchorKind.TAIL:
            reader.seek_tail()
        else:
            if query.after_realtime_usec is not None:
                slack = query.realtime_slack_usec
                after = query.after_realtime_usec
                reader.seek_realtime_usec(after - slack if after > slack else 0)
            else:
                reader.seek_head()
    else:
        if anchor.kind == ExplorerAnchorKind.REALTIME:
            reader.seek_realtime_usec(anchor.realtime_usec)
        elif anchor.kind == ExplorerAnchorKind.HEAD:
            reader.seek_head()
        else:
            if query.before_realtime_usec is not None:
                slack = query.realtime_slack_usec
                reader.seek_realtime_usec(query.before_realtime_usec + slack)
            else:
                reader.seek_tail()


def _step_explorer(reader, direction):
    return reader.next() if direction == Direction.FORWARD else reader.previous()


def _current_explorer_row(reader, realtime_usec, stats, expand=True):
    cursor = reader.get_cursor()
    if not expand:
        return ExplorerRow(realtime_usec=int(realtime_usec), cursor=cursor or '', payloads=[])
    payloads = reader.collect_entry_payloads()
    stats.returned_row_expansions += 1
    return ExplorerRow(realtime_usec=int(realtime_usec), cursor=cursor or '', payloads=payloads)


# ----------------------------------------------------------------------------
# Row scan: walk DATA objects, classify each, and apply matching values.
# Compressed DATA stays compressed unless the value is needed for
# filtering/faceting/FTS/display (AGENTS.md perf contract).
# ----------------------------------------------------------------------------


def _scan_row_data(reader, query, accumulator, row_id, apply, stats, needs_fts):
    """Scan a row's DATA objects and apply matching ones. Mirrors Rust
    `scan_current_row` (L2038-2094) and the `apply: ScanApply<'_>`
    parameter (L2043). Pass `_SCAN_APPLY_IMMEDIATE` for the
    immediate-apply fast path (Rust `ScanApply::Immediate`, L2169);
    pass a `_ScanApplyDeferred(deferred=...)` whose `deferred` list
    the caller drains after the row is fully classified.
    Always returns `(fts_matches, fts_negative)` like Rust's
    `RowScan::default()`; the fast-path early return uses the same
    default values.
    """

    # Mirror Rust scan_current_row L2046: every visited row counts.
    stats.rows_examined += 1
    if accumulator.required_identity_count == 0 and not needs_fts:
        return (False, False)
    fts_matches = False
    fts_negative = False
    for data_offset in reader._current_entry_data_offsets():
        stats.data_refs_seen += 1
        class_value = accumulator.offset_cache.lookup(data_offset)
        if class_value is None:
            stats.data_cache_misses += 1
            try:
                payload = reader._read_data_payload_at(data_offset)
            except Exception:
                continue
            stats.data_payloads_loaded += 1
            if reader._data_object_was_compressed(data_offset):
                stats.payloads_decompressed += 1
            split = _split_payload(payload)
            if split is None:
                if needs_fts:
                    stats.fts_scans += 1
                    pos, neg = _match_fts_query(payload, query)
                    if neg:
                        class_value = _OFFSET_CLASS_FTS_NEGATIVE
                        fts_negative = True
                    elif pos:
                        class_value = _OFFSET_CLASS_FTS_MATCH
                        fts_matches = True
                    else:
                        class_value = _OFFSET_CLASS_IRRELEVANT
                else:
                    class_value = _OFFSET_CLASS_IRRELEVANT
                accumulator.offset_cache.insert(data_offset, class_value)
                stats.data_objects_classified += 1
                continue
            field, value = split
            if needs_fts:
                stats.fts_scans += 1
                pos, neg = _match_fts_query(value, query)
            else:
                pos, neg = False, False
            if neg:
                class_value = _OFFSET_CLASS_FTS_NEGATIVE
                fts_negative = True
            elif field in accumulator.field_lookup:
                field_index = accumulator.field_lookup[field]
                value_index = accumulator.add_value(field_index, value, pos)
                class_value = _OFFSET_CLASS_VALUE_BASE + value_index
            elif pos:
                class_value = _OFFSET_CLASS_FTS_MATCH
                fts_matches = True
            else:
                class_value = _OFFSET_CLASS_IRRELEVANT
            accumulator.offset_cache.insert(data_offset, class_value)
            stats.data_objects_classified += 1
            if pos:
                fts_matches = True
            if neg:
                fts_negative = True
            if class_value >= _OFFSET_CLASS_VALUE_BASE:
                _handle_value_class(accumulator, class_value - _OFFSET_CLASS_VALUE_BASE, row_id, query, apply, stats)
            continue
        stats.data_cache_hits += 1
        if class_value == _OFFSET_CLASS_IRRELEVANT:
            stats.data_refs_skipped += 1
        elif class_value == _OFFSET_CLASS_FTS_NEGATIVE:
            fts_negative = True
        elif class_value == _OFFSET_CLASS_FTS_MATCH:
            fts_matches = True
        elif class_value >= _OFFSET_CLASS_VALUE_BASE:
            _handle_value_class(accumulator, class_value - _OFFSET_CLASS_VALUE_BASE, row_id, query, apply, stats)
    return fts_matches, fts_negative


def _handle_value_class(accumulator, value_index, row_id, query, apply, stats):
    """Mirror Rust `handle_row_value_class` (L2611-2643).

    The `apply` argument is the Python analogue of Rust's
    `ScanApply<'a>` enum (L2169-2172). `Immediate` calls
    `apply_value(value_index, None, stats)` during the scan; `Deferred`
    pushes the value_index into a list the caller drains after the row
    has been fully classified. This matches the Rust semantics
    exactly: time-bounded and FTS paths always defer so the caller
    can apply the histogram with the real effective timestamp; the
    facet scan's no-bound fast path applies inline.
    """

    field_index = accumulator.value_field_indices[value_index]
    use_first_value = query.field_mode == ExplorerFieldMode.FIRST_VALUE
    flags = accumulator.flags[field_index]
    is_required_role = (flags & (_FACET_PUBLIC | _FACET_HISTOGRAM)) != 0
    first_for_field = True
    if use_first_value or is_required_role:
        first_for_field = accumulator.mark_field_seen(field_index, row_id)
    if use_first_value and not first_for_field:
        return
    if isinstance(apply, _ScanApplyImmediate):
        accumulator.apply_value(value_index, None, stats)
        return
    apply.deferred.append((value_index, accumulator.value_source_realtime[value_index]))


@dataclass
class _ScanApplyImmediate:
    """Rust `ScanApply::Immediate` analogue: apply the value inline."""

    pass


@dataclass
class _ScanApplyDeferred:
    """Rust `ScanApply::Deferred(&mut Vec<usize>)` analogue: caller
    drains the deferred list and applies with a real timestamp.
    """

    deferred: List  # List[Tuple[int, Optional[int]]]


_SCAN_APPLY_IMMEDIATE = _ScanApplyImmediate()


def _pick_source_realtime(deferred):
    for _, source_realtime in deferred:
        if source_realtime is not None and source_realtime != 0:
            return source_realtime
    return None


# ----------------------------------------------------------------------------
# Traversal strategy entry point. Splits into combined or split passes.
# ----------------------------------------------------------------------------


def _explore_file_reader(reader, query, strategy, control):
    """Top-level dispatcher mirroring FileReader::explore_with_strategy_and_control (L1215-1228)."""

    _validate_no_debug_column_collection(query)
    if strategy == ExplorerStrategy.TRAVERSAL:
        return _explore_traversal(reader, query, control)
    if strategy == ExplorerStrategy.INDEX:
        return _explore_indexed(reader, query, control)
    if strategy == ExplorerStrategy.COMPARE:
        return _explore_compare(reader, query)
    raise ExplorerError(f'unsupported explorer strategy {strategy!r}')


def _explore_traversal(reader, query, control):
    _validate_query(query)
    result = _explorer_result_for_query(reader, query)
    groups = _facet_pass_groups(query)
    if _can_run_combined_pass(groups):
        _explore_traversal_combined(reader, query, groups, result, control)
    else:
        _explore_traversal_split(reader, query, groups, result, control)
    _flush_reader_filters(reader)
    return result


def _explorer_result_for_query(reader, query):
    result = ExplorerResult()
    result.column_fields = set(_enumerate_fields_indexed(reader))
    if query.histogram is not None:
        result.histogram = _new_histogram(query.histogram, query)
    return result


def _explore_traversal_combined(reader, query, groups, result, control):
    facet_indices = _combined_facet_indices(groups)
    if not _query_needs_main_pass(query) and not facet_indices:
        return
    _configure_filters(reader, query, None)
    accumulator = _build_combined_accumulator(query, facet_indices, result.histogram)
    _scan_explorer_combined(reader, query, accumulator, result, bool(facet_indices), control)
    accumulator.finish_facets(result)
    accumulator.finish_histogram(result.histogram)


def _explore_traversal_split(reader, query, groups, result, control):
    if _query_needs_main_pass(query):
        _configure_filters(reader, query, None)
        accumulator = _build_main_accumulator(query, result.histogram)
        _scan_explorer_main(reader, query, accumulator, result, control)
        accumulator.finish_histogram(result.histogram)
    for group in groups:
        if control is not None and control._stopped:
            break
        _configure_filters(reader, query, group.excluded_field)
        accumulator = _build_facet_accumulator(query, group.facet_indices, _facet_pass_needs_source_realtime(query))
        _scan_explorer_facet(reader, query, accumulator, result.stats, control)
        accumulator.finish_facets(result)


def _build_main_accumulator(query, histogram):
    acc = _ExplorerAccumulator(histogram)
    if query.histogram is not None:
        acc.add_field(query.histogram, _FACET_HISTOGRAM)
    if _query_needs_source_realtime_main(query):
        acc.add_field(SOURCE_REALTIME_FIELD, _FACET_SOURCE_REALTIME)
    return acc


def _build_facet_accumulator(query, facet_indices, include_source_realtime):
    acc = _ExplorerAccumulator(None)
    for idx in facet_indices:
        if 0 <= idx < len(query.facets):
            acc.add_field(query.facets[idx], _FACET_PUBLIC)
    if include_source_realtime:
        acc.add_field(SOURCE_REALTIME_FIELD, _FACET_SOURCE_REALTIME)
    return acc


def _build_combined_accumulator(query, facet_indices, histogram):
    acc = _ExplorerAccumulator(histogram)
    if query.histogram is not None:
        acc.add_field(query.histogram, _FACET_HISTOGRAM)
    for idx in facet_indices:
        if 0 <= idx < len(query.facets):
            acc.add_field(query.facets[idx], _FACET_PUBLIC)
    if _query_needs_source_realtime_main(query) or _facet_pass_needs_source_realtime(query):
        acc.add_field(SOURCE_REALTIME_FIELD, _FACET_SOURCE_REALTIME)
    return acc


def _scan_explorer_main(reader, query, accumulator, result, control):
    """Mirror Rust scan_explorer_main (L1841-1900)."""

    _seek_for_explorer(reader, query)
    use_first_value = query.field_mode == ExplorerFieldMode.FIRST_VALUE
    needs_fts = _query_has_fts(query)
    row_id = 0
    rows_seen = 0
    apply = _ScanApplyDeferred(deferred=[])
    missing = accumulator.required_identity_count if use_first_value else 0
    while True:
        if not _step_explorer(reader, query.direction):
            break
        rows_seen += 1
        if control is not None and control.should_stop_after_rows(rows_seen, result.stats):
            break
        commit_realtime = reader.get_realtime_usec()
        if _stop_by_commit_time(query, commit_realtime):
            break
        if _skip_by_commit_time(query, commit_realtime):
            continue
        if not _reader_filter_matches(reader):
            continue
        apply.deferred.clear()
        row_id += 1
        fts_match, fts_negative = _scan_row_data(reader, query, accumulator, row_id, apply, result.stats, needs_fts)
        source_realtime = _pick_source_realtime(apply.deferred)
        effective = _effective_realtime_from_scan(source_realtime, commit_realtime)
        _record_source_realtime_delta(result.stats, source_realtime, commit_realtime)
        if not _timestamp_in_range(query, effective):
            continue
        if _row_rejected_by_fts(query, fts_match, fts_negative):
            continue
        _record_last_realtime(result.stats, commit_realtime)
        result.stats.rows_matched += 1
        stop_after_matched = False
        if control is not None:
            stop_after_matched = control.emit_matched_row(effective, result.stats.rows_matched)
        value_realtime = effective if query.histogram is not None else None
        for value_index, _ in apply.deferred:
            accumulator.apply_value(value_index, value_realtime, result.stats)
        accumulator.finish_histogram_row(row_id, effective, result.stats)
        if _row_within_anchor(query, effective) and len(result.rows) < query.limit:
            result.rows.append(_current_explorer_row(reader, effective, result.stats, expand=True))
        if stop_after_matched or _should_stop_when_rows_full(query, result.rows, effective, result.stats.rows_matched):
            break
        if use_first_value and not needs_fts and missing == 0 and not apply.deferred:
            result.stats.early_stop_opportunities += 1
            result.stats.early_stops += 1
    result.stats.rows_returned = len(result.rows)


def _scan_explorer_combined(reader, query, accumulator, result, include_facets, control):
    """Mirror Rust scan_explorer_combined (L1902-1981)."""

    _seek_for_explorer(reader, query)
    use_first_value = query.field_mode == ExplorerFieldMode.FIRST_VALUE
    needs_fts = _query_has_fts(query)
    row_id = 0
    rows_seen = 0
    apply = _ScanApplyDeferred(deferred=[])
    while True:
        if not _step_explorer(reader, query.direction):
            break
        rows_seen += 1
        if control is not None and control.should_stop_after_rows(rows_seen, result.stats):
            break
        commit_realtime = reader.get_realtime_usec()
        if _stop_by_commit_time(query, commit_realtime):
            break
        if _skip_by_commit_time(query, commit_realtime):
            continue
        if not _reader_filter_matches(reader):
            continue
        apply.deferred.clear()
        row_id += 1
        fts_match, fts_negative = _scan_row_data(reader, query, accumulator, row_id, apply, result.stats, needs_fts)
        source_realtime = _pick_source_realtime(apply.deferred)
        effective = _effective_realtime_from_scan(source_realtime, commit_realtime)
        _record_source_realtime_delta(result.stats, source_realtime, commit_realtime)
        if not _timestamp_in_range(query, effective):
            continue
        if _row_rejected_by_fts(query, fts_match, fts_negative):
            continue
        _record_last_realtime(result.stats, commit_realtime)
        stop_after_matched = False
        if _query_needs_main_pass(query):
            result.stats.rows_matched += 1
            if control is not None:
                stop_after_matched = control.emit_matched_row(effective, result.stats.rows_matched)
        if include_facets:
            result.stats.facet_rows_matched += 1
        value_realtime = effective if query.histogram is not None else None
        for value_index, _ in apply.deferred:
            accumulator.apply_value(value_index, value_realtime, result.stats)
        if query.histogram is not None:
            accumulator.finish_histogram_row(row_id, effective, result.stats)
        if include_facets:
            accumulator.finish_facet_row(row_id, result.stats)
        if (
            _query_needs_main_pass(query)
            and _row_within_anchor(query, effective)
            and len(result.rows) < query.limit
        ):
            result.rows.append(_current_explorer_row(reader, effective, result.stats, expand=True))
        if stop_after_matched or _should_stop_when_rows_full(query, result.rows, effective, result.stats.rows_matched):
            break
    result.stats.rows_returned = len(result.rows)


def _scan_explorer_facet(reader, query, accumulator, stats, control):
    """Mirror Rust scan_explorer_facet (L1983-2036)."""

    _seek_for_explorer(reader, query)
    needs_fts = _query_has_fts(query)
    defer_apply = (
        query.after_realtime_usec is not None
        or query.before_realtime_usec is not None
        or needs_fts
    )
    row_id = 0
    rows_seen = 0
    apply = (
        _ScanApplyDeferred(deferred=[])
        if defer_apply
        else _SCAN_APPLY_IMMEDIATE
    )
    while True:
        if not _step_explorer(reader, query.direction):
            break
        rows_seen += 1
        if control is not None and control.should_stop_after_rows(rows_seen, stats):
            break
        commit_realtime = reader.get_realtime_usec()
        if _stop_by_commit_time(query, commit_realtime):
            break
        if _skip_by_commit_time(query, commit_realtime):
            continue
        if not _reader_filter_matches(reader):
            continue
        if isinstance(apply, _ScanApplyDeferred):
            apply.deferred.clear()
        row_id += 1
        fts_match, fts_negative = _scan_row_data(reader, query, accumulator, row_id, apply, stats, needs_fts)
        if isinstance(apply, _ScanApplyDeferred):
            source_realtime = _pick_source_realtime(apply.deferred)
        else:
            source_realtime = None
        effective = _effective_realtime_from_scan(source_realtime, commit_realtime)
        _record_source_realtime_delta(stats, source_realtime, commit_realtime)
        if not _timestamp_in_range(query, effective):
            continue
        if _row_rejected_by_fts(query, fts_match, fts_negative):
            continue
        _record_last_realtime(stats, commit_realtime)
        stats.facet_rows_matched += 1
        if isinstance(apply, _ScanApplyDeferred):
            for value_index, _ in apply.deferred:
                accumulator.apply_value(value_index, None, stats)
        accumulator.finish_facet_row(row_id, stats)


# ----------------------------------------------------------------------------
# Index strategy: derive candidate entry offsets via the FIELD chain,
# then count facets and histogram values without full row data.
# ----------------------------------------------------------------------------


def _explore_indexed(reader, query, control):
    """Mirror Rust explore_indexed (L1417-1431)."""

    _validate_query(query)
    _validate_indexed_query(query)
    result = _explorer_result_for_query(reader, query)
    candidates = _indexed_candidate_set(reader, query, None)
    if control is not None and control._stopped:
        _flush_reader_filters(reader)
        return result
    _indexed_collect_rows(reader, query, result, candidates, control)
    if control is not None and control._stopped:
        _flush_reader_filters(reader)
        return result
    _indexed_collect_facets(reader, query, result, candidates, control)
    _indexed_collect_histogram(reader, query, result, candidates, control)
    _flush_reader_filters(reader)
    return result


def _explore_compare(reader, query):
    """Compare runs both Traversal and Index and verifies equality (Rust L1390-1415)."""

    traversal_started = time.monotonic()
    traversal = _explore_traversal(reader, query, None)
    traversal_duration = time.monotonic() - traversal_started

    index_started = time.monotonic()
    indexed = _explore_indexed(reader, query, None)
    index_duration = time.monotonic() - index_started

    if not _explorer_outputs_match(traversal, indexed):
        raise ExplorerError('indexed explorer output differs from traversal explorer output')

    indexed.comparison = ExplorerComparison(
        traversal_duration=traversal_duration,
        index_duration=index_duration,
        traversal_stats=traversal.stats,
        index_stats=indexed.stats.copy(),
    )
    return indexed


def _explorer_outputs_match(left, right):
    if len(left.rows) != len(right.rows):
        return False
    for a, b in zip(left.rows, right.rows):
        if a.realtime_usec != b.realtime_usec or a.cursor != b.cursor or a.payloads != b.payloads:
            return False
    if left.facets != right.facets:
        return False
    return _histograms_match(left.histogram, right.histogram)


def _histograms_match(left, right):
    if left is None and right is None:
        return True
    if left is None or right is None:
        return False
    if left.field != right.field or len(left.buckets) != len(right.buckets):
        return False
    for a, b in zip(left.buckets, right.buckets):
        if (
            a.start_realtime_usec != b.start_realtime_usec
            or a.end_realtime_usec != b.end_realtime_usec
            or a.values != b.values
        ):
            return False
    return True


def _indexed_candidate_set(reader, query, excluded_field):
    """Derive candidate entry offsets for the indexed strategy.

    Mirrors Rust indexed_candidate_set (L1489-1526). If no filters
    and no time bound are active, every entry is a candidate.
    Otherwise the reader's filter builder + entry-array walk
    produces the candidate set. Returns a sorted list of entry
    offsets; the caller walks it according to `query.direction`.
    """

    has_active_filter = any(
        (excluded_field is None or f.field != excluded_field) and f.values
        for f in query.filters
    )
    has_time_bound = query.after_realtime_usec is not None or query.before_realtime_usec is not None
    if not has_active_filter and not has_time_bound:
        return list(reader._entry_offsets)
    _configure_filters(reader, query, excluded_field)
    _seek_for_explorer(reader, query)
    offsets = []
    seen = set()
    while _step_explorer(reader, query.direction):
        commit_realtime = reader.get_realtime_usec()
        if _stop_by_commit_time(query, commit_realtime):
            break
        if _skip_by_commit_time(query, commit_realtime):
            continue
        if 0 <= reader._entry_index < len(reader._entry_offsets):
            entry_offset = reader._entry_offsets[reader._entry_index]
            if entry_offset not in seen:
                seen.add(entry_offset)
                offsets.append(entry_offset)
    _flush_reader_filters(reader)
    return offsets


def _indexed_collect_rows(reader, query, result, candidates, control):
    if query.limit == 0:
        return
    for entry_offset in candidates:
        if control is not None and control._stopped:
            break
        index = reader._index_for_entry_offset(entry_offset)
        if index is None:
            continue
        reader._position_at_index(index, query.direction)
        commit_realtime = reader.get_realtime_usec()
        if _stop_by_commit_time(query, commit_realtime):
            return
        if _skip_by_commit_time(query, commit_realtime):
            continue
        if not _timestamp_in_range(query, commit_realtime):
            continue
        _record_last_realtime(result.stats, commit_realtime)
        result.stats.rows_matched += 1
        if control is not None and control.emit_matched_row(commit_realtime, result.stats.rows_matched):
            break
        if _row_within_anchor(query, commit_realtime) and len(result.rows) < query.limit:
            result.rows.append(_current_explorer_row(reader, commit_realtime, result.stats, expand=True))
    if query.direction == Direction.BACKWARD:
        result.rows.sort(key=lambda r: r.realtime_usec, reverse=True)
    else:
        result.rows.sort(key=lambda r: r.realtime_usec)
    if len(result.rows) > query.limit:
        result.rows = result.rows[:query.limit]


def _indexed_collect_facets(reader, query, result, candidates, control):
    if control is not None and control._stopped:
        return
    for group in _facet_pass_groups(query):
        group_candidates = candidates
        if group.excluded_field is not None:
            group_candidates = _indexed_candidate_set(reader, query, group.excluded_field)
        result.stats.facet_rows_matched += len(group_candidates)
        for facet_index in group.facet_indices:
            if facet_index >= len(query.facets):
                continue
            field = query.facets[facet_index]
            _indexed_count_facet_field(reader, field, group_candidates, result, query, control)
            if control is not None and control._stopped:
                return


def _indexed_collect_histogram(reader, query, result, candidates, control):
    if query.histogram is None:
        return
    if control is not None and control._stopped:
        return
    _indexed_count_histogram(reader, query.histogram, candidates, result, query, control)


def _indexed_count_facet_field(reader, field, candidates, result, query, control):
    """Walk the FIELD chain for `field` and count entries that are
    candidates. Mirrors Rust indexed_count_facet_group (L2294-2348).
    """

    values = {}
    rows_with_field = set()
    field_offset = reader._find_field_head_data_offset(field)
    while field_offset:
        if control is not None and control._stopped:
            break
        try:
            data_header = reader._read_data_header_at(field_offset)
            payload = reader._read_data_payload_at(field_offset)
        except Exception:
            break
        result.stats.data_objects_classified += 1
        result.stats.data_payloads_loaded += 1
        if reader._data_object_was_compressed(field_offset):
            result.stats.payloads_decompressed += 1
        split = _split_payload(payload)
        if split is None or split[0] != field:
            field_offset = data_header['next_field_offset']
            continue
        value = split[1]
        count = 0
        for entry_offset in _data_entry_offsets(reader, data_header):
            result.stats.data_refs_seen += 1
            if entry_offset in candidates:
                count += 1
                rows_with_field.add(entry_offset)
        if count:
            _increment_counter(values, value, count)
            result.stats.facet_updates += count
        field_offset = data_header['next_field_offset']
    unset = len(candidates) - len(rows_with_field)
    if unset:
        _increment_counter(values, UNSET_VALUE, unset)
        result.stats.facet_updates += unset
    result.facets[field] = values


def _indexed_count_histogram(reader, field, candidates, result, query, control):
    histogram = result.histogram
    if histogram is None or not histogram.buckets:
        return
    histogram_start = histogram.buckets[0].start_realtime_usec
    width = histogram.buckets[0].end_realtime_usec - histogram.buckets[0].start_realtime_usec
    if width <= 0:
        width = 1
    bucket_count = len(histogram.buckets)
    rows_with_field = set()
    field_offset = reader._find_field_head_data_offset(field)
    while field_offset:
        if control is not None and control._stopped:
            break
        try:
            data_header = reader._read_data_header_at(field_offset)
            payload = reader._read_data_payload_at(field_offset)
        except Exception:
            break
        result.stats.data_objects_classified += 1
        result.stats.data_payloads_loaded += 1
        if reader._data_object_was_compressed(field_offset):
            result.stats.payloads_decompressed += 1
        split = _split_payload(payload)
        if split is None or split[0] != field:
            field_offset = data_header['next_field_offset']
            continue
        value = split[1]
        for entry_offset in _data_entry_offsets(reader, data_header):
            result.stats.data_refs_seen += 1
            if entry_offset not in candidates:
                continue
            rows_with_field.add(entry_offset)
            commit_realtime = reader._entry_realtime_at_offset(entry_offset)
            if not _timestamp_in_range(query, commit_realtime):
                continue
            idx = _histogram_bucket_index_from_bounds(commit_realtime, histogram_start, width, bucket_count)
            if idx is None:
                continue
            bucket = histogram.buckets[idx]
            _increment_counter(bucket.values, value, 1)
            result.stats.histogram_updates += 1
        field_offset = data_header['next_field_offset']
    for entry_offset in candidates:
        if entry_offset in rows_with_field:
            continue
        commit_realtime = reader._entry_realtime_at_offset(entry_offset)
        if not _timestamp_in_range(query, commit_realtime):
            continue
        idx = _histogram_bucket_index_from_bounds(commit_realtime, histogram_start, width, bucket_count)
        if idx is None:
            continue
        bucket = histogram.buckets[idx]
        _increment_counter(bucket.values, UNSET_VALUE, 1)
        result.stats.histogram_updates += 1


def _data_entry_offsets(reader, data_header):
    """Yield every entry offset this DATA object references. Mirrors
    Go `visitDataEntryOffsets` (L1173-1198) and the Rust equivalent.
    """

    n_entries = data_header.get('n_entries', 0) or 0
    if n_entries == 0:
        return
    entry_offset = data_header.get('entry_offset', 0) or 0
    if entry_offset:
        yield entry_offset
        n_entries -= 1
    array_offset = data_header.get('entry_array_offset', 0) or 0
    while array_offset and n_entries > 0:
        try:
            array = reader._read_entry_array_object(array_offset)
        except Exception:
            return
        if array is None:
            return
        capacity = array['capacity']
        for i in range(min(n_entries, capacity)):
            off = reader._read_entry_array_item_offset(array['data_start'] + i * array['item_size'])
            if off:
                yield off
        n_entries -= min(n_entries, capacity)
        array_offset = array['next_offset']


# ----------------------------------------------------------------------------
# Multi-file scan: run the explorer against each file and merge the
# results. Per-file facets, histograms, and column_fields are merged;
# rows are merged and re-sorted by realtime according to the query
# direction, then truncated to `query.limit`. Compare-strategy is not
# supported across files (it needs a single-file equal-output check).
#
# INTERNAL: this helper is module-private. In Rust, multi-file exploration
# lives on the Netdata-layer `explore_files` (see
# `rust/src/journal/src/netdata.rs:467`). The corresponding Python
# Netdata layer is planned for the next chunk of SOW-0104, so this
# helper stays underscore-prefixed here until the Netdata port consumes
# it. Do NOT expose it via any public reader API: the contract is that
# `DirectoryReader` and `FileReader` only have single-file `explore*`
# methods, matching Rust's file-reader placement.
# ----------------------------------------------------------------------------


def _explore_files(readers, query, strategy, control):
    """Run an explorer query across a list of single-file readers.

    Internal helper for the upcoming Netdata-layer multi-file
    `explore_files` port; not exposed via any public reader API. Per-file
    semantics are identical to the single-file path; the merge keeps
    the per-file `column_fields` union and the union of all
    facet/histogram values (with sums).
    """

    if control is not None:
        _validate_no_debug_column_collection(query)
    else:
        _validate_no_debug_column_collection(query)
    if strategy == ExplorerStrategy.COMPARE:
        raise ExplorerError(
            'Compare strategy requires a single-file reader; the directory reader '
            'does not support logical-equality verification across files.'
        )
    if strategy == ExplorerStrategy.INDEX:
        _validate_indexed_query(query)
    rows = []
    merged_facets = {}
    merged_histogram = None
    column_fields = set()
    last_stats = None
    for reader in readers:
        if control is not None and control._stopped:
            break
        sub = _explore_file_reader(reader, query, strategy, control)
        if sub is None:
            continue
        rows.extend(sub.rows)
        for field, values in sub.facets.items():
            dest = merged_facets.setdefault(field, {})
            for value, count in values.items():
                _increment_counter(dest, value, count)
        if sub.histogram is not None:
            if merged_histogram is None:
                merged_histogram = ExplorerHistogram(field=sub.histogram.field, buckets=[
                    ExplorerHistogramBucket(
                        start_realtime_usec=b.start_realtime_usec,
                        end_realtime_usec=b.end_realtime_usec,
                    )
                    for b in sub.histogram.buckets
                ])
            for bucket_idx, sub_bucket in enumerate(sub.histogram.buckets):
                if bucket_idx >= len(merged_histogram.buckets):
                    break
                for value, count in sub_bucket.values.items():
                    _increment_counter(merged_histogram.buckets[bucket_idx].values, value, count)
        column_fields.update(sub.column_fields)
        last_stats = sub.stats
    rows.sort(key=lambda r: r.realtime_usec, reverse=(query.direction == Direction.BACKWARD))
    if query.limit and len(rows) > query.limit:
        rows = rows[:query.limit]
    result = ExplorerResult(
        rows=rows,
        facets=merged_facets,
        histogram=merged_histogram,
        column_fields=column_fields,
    )
    if last_stats is not None:
        result.stats = last_stats
    result.stats.rows_returned = len(rows)
    return result
