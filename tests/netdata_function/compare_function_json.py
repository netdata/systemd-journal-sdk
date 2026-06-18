#!/usr/bin/env python3
"""Compare Netdata function JSON content at the SDK/plugin boundary.

This comparator is strict about user-visible journal content, but it does not
treat dictionary/list emission order as content when Netdata itself derives that
order from hash-table traversal. It compares the complete table column catalog,
every returned row value by column name, all content facet fields and values by
id, and histogram buckets by timestamp and value label.

Netdata can also emit diagnostic/accounting values that are not journal content:
`items.evaluated` counts internal scan work, and the facets compatibility layer
can emit a zero-count hash id for an unavailable empty unique value. These are
reported, but they do not decide content equality. Data-only responses can also
expose all-null plugin column-catalog artifacts from rows that were scanned for
paging but not returned; those are reported as non-content unless a column has
any returned-row value on either side.
"""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


VOLATILE_TOP_LEVEL_FIELDS = (
    "_journal_files",
    "_stats",
    "_fstat_caching",
    "_sampling",
    "expires",
    "help",
    "last_modified",
    "message",
    "versions",
)

VOLATILE_COLUMN_METADATA_FIELDS = {
    "index",
}

VOLATILE_FACET_METADATA_FIELDS = {
    "order",
}

NETDATA_EMPTY_STRING_FACET_HASH_ID = "CzGfAU2z3TC"
NETDATA_UNAVAILABLE_FIELD_LABEL = "[unavailable field]"

DIAGNOSTIC_ITEM_FIELDS = {
    "evaluated",
}


# The source-option `info` string embeds live-volatile components
# (`covering <duration>` and `last entry at <iso>`) derived from the tail
# of the journal at the time the peer ran the request. With a third
# peer, the slow peer can see a tail several seconds newer than the
# fast peers; observed up to ~6s while SDK and plugin agree to the
# second. The 2-peer design relied on back-to-back invocations.
#
# The comparator accepts a bounded skew for those two components only;
# file counts and total sizes stay strict. `off` and `unknown` literals
# only equal themselves.
SOURCE_INFO_SKEW_TOLERANCE_SECONDS = 300

# SOW-0104 fix-10: the `_request.after` / `_request.before` echoes
# embed parse-time `unix_now_seconds()` by reference design
# (Rust L1418 -> L3624-3690). Two peers invoked seconds apart
# legitimately produce different echoes; the slow third peer is
# not a real content mismatch. The comparator accepts a bounded
# skew on those two echoes ONLY. Other `_request` fields stay
# strict. Mirrors the fix-4 source-info tolerance precedent.
REQUEST_WINDOW_SKEW_TOLERANCE_SECONDS = 300

_INFO_STRING_PATTERN = re.compile(
    r"^(?P<files>\d+) files, total size (?P<size>[^,]+), "
    r"covering (?P<covering>(?:[^,]*))(?:, last entry at "
    r"(?P<last_entry>[^,]*))?$"
)
_COVERING_DURATION_UNIT_SECONDS = {
    "y": 365 * 86_400,
    "mo": 30 * 86_400,
    "d": 86_400,
    "h": 3600,
    "m": 60,
    "s": 1,
}
_DURATION_COMPONENT_PATTERN = re.compile(r"(\d+)(y|mo|d|h|m|s)")
_DURATION_TOKEN_PATTERN = re.compile(
    r"^(?:\d+y|\d+mo|\d+d|\d+h|\d+m|\d+s)(?: (?:\d+y|\d+mo|\d+d|\d+h|\d+m|\d+s))*$"
)
_RFC3339_UTC_Z_PATTERN = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$"
)


def parse_source_info_string(info: str) -> dict[str, Any] | None:
    """Parse the `N files, total size S, covering C, last entry at T` shape.

    Returns a structured representation on success and ``None`` if the
    string does not match the shape. The returned dict always has
    ``files`` and ``size`` (strings, compared exactly). ``covering`` and
    ``last_entry`` are tuples of ``(kind, value_seconds_or_none)`` or
    ``(literal_string,)`` for `off`/`unknown` so equality can compare
    the parsed values while preserving the exact-equality rule for the
    literal fallbacks.
    """

    if not isinstance(info, str):
        return None
    match = _INFO_STRING_PATTERN.match(info)
    if not match:
        return None
    files = match.group("files")
    size = match.group("size")
    covering_token = match.group("covering").strip()
    last_entry_token = match.group("last_entry")
    last_entry_token = last_entry_token.strip() if last_entry_token is not None else None
    return {
        "files": files,
        "size": size,
        "covering": _parse_covering_token(covering_token),
        "last_entry": _parse_last_entry_token(last_entry_token),
    }


def _parse_covering_token(raw: str) -> tuple[Any, ...]:
    if raw == "off":
        return ("off",)
    seconds = _parse_duration_seconds(raw)
    if seconds is None:
        return ("raw", raw)
    return ("seconds", seconds)


def _parse_last_entry_token(raw: str | None) -> tuple[Any, ...]:
    if raw is None or raw == "unknown":
        return ("unknown",)
    seconds = _parse_rfc3339_utc_seconds(raw)
    if seconds is None:
        return ("raw", raw)
    return ("seconds", seconds)


def _parse_duration_seconds(token: str) -> int | None:
    if not token:
        return None
    if not _DURATION_TOKEN_PATTERN.match(token):
        return None
    components = _DURATION_COMPONENT_PATTERN.findall(token)
    total = 0
    for count, unit in components:
        total += int(count) * _COVERING_DURATION_UNIT_SECONDS[unit]
    return total


def _parse_rfc3339_utc_seconds(token: str) -> int | None:
    if not _RFC3339_UTC_Z_PATTERN.match(token):
        return None
    iso = token[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(iso)
    except ValueError:
        return None
    if parsed.tzinfo != timezone.utc:
        return None
    return int(parsed.timestamp())


def source_info_equal_with_skew(
    left: Any, right: Any
) -> tuple[bool, dict[str, Any] | None]:
    """Compare two `info` strings, tolerating bounded skew on the
    live-volatile components.

    Returns ``(equal, diagnostic)``. ``diagnostic`` is ``None`` when no
    tolerance was applied; otherwise it carries the field name, parsed
    left/right values, the delta in seconds, and the bound. The
    diagnostic is intended for the comparison's non-content section.
    """

    if left == right:
        return True, None
    left_parsed = parse_source_info_string(left)
    right_parsed = parse_source_info_string(right)
    if left_parsed is None or right_parsed is None:
        return False, None
    if (
        left_parsed["files"] != right_parsed["files"]
        or left_parsed["size"] != right_parsed["size"]
    ):
        return False, None
    skews: dict[str, dict[str, Any]] = {}
    for field in ("covering", "last_entry"):
        equal, diagnostic = _compare_skew_field(
            left_parsed[field], right_parsed[field]
        )
        if not equal:
            return False, None
        if diagnostic is not None:
            skews[field] = diagnostic
    if not skews:
        # The strings differed but parsed to identical structures;
        # keep strict comparison for that case.
        return False, None
    return True, {
        "skew_bound_seconds": SOURCE_INFO_SKEW_TOLERANCE_SECONDS,
        "fields": skews,
    }


def _compare_skew_field(
    left: tuple[Any, ...], right: tuple[Any, ...]
) -> tuple[bool, dict[str, Any] | None]:
    if left == right:
        return True, None
    if left[0] != "seconds" or right[0] != "seconds":
        return False, None
    delta = abs(int(left[1]) - int(right[1]))
    if delta > SOURCE_INFO_SKEW_TOLERANCE_SECONDS:
        return False, None
    return True, {
        "left_seconds": int(left[1]),
        "right_seconds": int(right[1]),
        "delta_seconds": delta,
        "bound_seconds": SOURCE_INFO_SKEW_TOLERANCE_SECONDS,
    }


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise SystemExit(f"{path}: top-level JSON is not an object")
    return value


def is_data_only(doc: dict[str, Any]) -> bool:
    request = doc.get("_request")
    return isinstance(request, dict) and request.get("data_only") is True


def column_indices(doc: dict[str, Any]) -> dict[str, int]:
    columns = doc.get("columns")
    if not isinstance(columns, dict):
        return {}
    out: dict[str, int] = {}
    for name, meta in columns.items():
        if isinstance(meta, dict) and isinstance(meta.get("index"), int):
            out[name] = meta["index"]
    return out


def columns_with_any_returned_value(doc: dict[str, Any]) -> set[str]:
    indices = column_indices(doc)
    rows = doc.get("data")
    if not isinstance(rows, list):
        return set()
    out: set[str] = set()
    for row in rows:
        if not isinstance(row, list):
            continue
        for name, idx in indices.items():
            if 0 <= idx < len(row) and row[idx] is not None:
                out.add(name)
    return out


def normalized_columns(
    doc: dict[str, Any], allowed_columns: set[str] | None = None
) -> dict[str, dict[str, Any]]:
    columns = doc.get("columns")
    if not isinstance(columns, dict):
        return {}
    out: dict[str, dict[str, Any]] = {}
    for name, meta in columns.items():
        if not isinstance(name, str) or not isinstance(meta, dict):
            continue
        if allowed_columns is not None and name not in allowed_columns:
            continue
        normalized = dict(meta)
        for field in VOLATILE_COLUMN_METADATA_FIELDS:
            normalized.pop(field, None)
        out[name] = normalize_json(normalized)
    return out


def normalized_rows(
    doc: dict[str, Any], allowed_columns: set[str] | None = None
) -> list[dict[str, Any]]:
    indices = column_indices(doc)
    rows = doc.get("data")
    if not isinstance(rows, list):
        return []
    if allowed_columns is None:
        ordered_columns = sorted(indices, key=lambda name: indices[name])
    else:
        ordered_columns = sorted(
            allowed_columns,
            key=lambda name: (indices.get(name, 1 << 30), name),
        )
    out: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, list):
            continue
        item: dict[str, Any] = {}
        for field in ordered_columns:
            idx = indices.get(field)
            if idx is not None and 0 <= idx < len(row):
                item[field] = normalize_row_value(field, row[idx])
            else:
                item[field] = None
        out.append(item)
    return out


def is_known_plugin_message_corruption(plugin_value: Any, sdk_value: Any) -> bool:
    """Detect a narrow installed-plugin MESSAGE corruption shape.

    The SDK side is intentionally not normalized here. The SDK must keep the
    journal file content that stock journalctl reports; the comparator only
    classifies this plugin-side defect so broader equality checks stay useful.
    """
    if not isinstance(plugin_value, str) or not isinstance(sdk_value, str):
        return False
    return (
        len(plugin_value) == len(sdk_value)
        and plugin_value.startswith("=")
        and "_CMDLINE=" in plugin_value
        and ": Executing command " in sdk_value
    )


def rows_match_with_known_plugin_message_corruption(
    left_rows: list[dict[str, Any]], right_rows: list[dict[str, Any]]
) -> tuple[bool, list[int]]:
    if left_rows == right_rows:
        return True, []
    if len(left_rows) != len(right_rows):
        return False, []

    ignored: list[int] = []
    for index, (left_row, right_row) in enumerate(zip(left_rows, right_rows)):
        if left_row == right_row:
            continue
        left_without_message = dict(left_row)
        right_without_message = dict(right_row)
        left_message = left_without_message.pop("MESSAGE", None)
        right_message = right_without_message.pop("MESSAGE", None)
        if left_without_message != right_without_message:
            return False, ignored
        if not is_known_plugin_message_corruption(left_message, right_message):
            return False, ignored
        ignored.append(index)
    return True, ignored


def normalize_row_value(field: str, value: Any) -> Any:
    if field == "ND_JOURNAL_FILE" and isinstance(value, str):
        return Path(value).name
    return normalize_json(value)


def raw_facets(doc: dict[str, Any]) -> list[Any]:
    facets = doc.get("facets")
    if not isinstance(facets, list):
        facets = doc.get("facets_delta")
    if not isinstance(facets, list):
        return []
    return facets


def ignored_unavailable_option(option_id: str, option: dict[str, Any]) -> bool:
    return (
        option_id == NETDATA_EMPTY_STRING_FACET_HASH_ID
        and option.get("name") == NETDATA_UNAVAILABLE_FIELD_LABEL
        and option.get("count") == 0
    )


def normalized_facet_options(facet: dict[str, Any]) -> dict[str, Any]:
    options: dict[str, Any] = {}
    raw_options = facet.get("options", [])
    if not isinstance(raw_options, list):
        return options
    for option in raw_options:
        if not isinstance(option, dict):
            continue
        option_id = option.get("id")
        if not isinstance(option_id, str) or ignored_unavailable_option(option_id, option):
            continue
        options[option_id] = normalize_json(
            {
                key: value
                for key, value in option.items()
                if key not in VOLATILE_FACET_METADATA_FIELDS
            }
        )
    return options


def normalized_facet(facet: dict[str, Any]) -> dict[str, Any]:
    normalized = {
        key: value
        for key, value in facet.items()
        if key not in {*VOLATILE_FACET_METADATA_FIELDS, "options"}
    }
    normalized["options"] = normalized_facet_options(facet)
    return normalize_json(normalized)


def normalized_facets(doc: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for facet in raw_facets(doc):
        if not isinstance(facet, dict):
            continue
        facet_id = facet.get("id")
        if isinstance(facet_id, str):
            out[facet_id] = normalized_facet(facet)
    return out


def request_selected_facet_ids(left: dict[str, Any], right: dict[str, Any]) -> set[str]:
    out: set[str] = set()
    for doc in (left, right):
        request = doc.get("_request")
        if not isinstance(request, dict):
            continue
        facets = request.get("facets")
        selections = request.get("selections")
        if not isinstance(facets, list) or not isinstance(selections, dict):
            continue
        requested = {facet for facet in facets if isinstance(facet, str)}
        selected = {field for field in selections if isinstance(field, str)}
        out |= requested & selected
    return out


def facets_match_with_selected_field_quirk(
    left: dict[str, Any],
    right: dict[str, Any],
    left_facets: dict[str, Any],
    right_facets: dict[str, Any],
) -> tuple[bool, list[str], list[dict[str, Any]]]:
    if left_facets == right_facets:
        return True, [], []
    skew_diagnostics: list[dict[str, Any]] = []
    if facets_equal_with_info_skew(
        left_facets, right_facets, skew_diagnostics
    ):
        return True, [], skew_diagnostics
    ignored = sorted(request_selected_facet_ids(left, right))
    if not ignored:
        return False, [], []
    ignored_set = set(ignored)
    left_filtered = selected_facet_options_removed(left_facets, ignored_set)
    right_filtered = selected_facet_options_removed(right_facets, ignored_set)
    if facets_equal_with_info_skew(left_filtered, right_filtered, skew_diagnostics):
        return True, ignored, skew_diagnostics
    return False, [], []


def facets_equal_with_info_skew(
    left_facets: dict[str, Any],
    right_facets: dict[str, Any],
    skew_diagnostics: list[dict[str, Any]],
) -> bool:
    """Compare facet structures with bounded skew tolerance on
    source-option `info` strings.

    Records any applied tolerance in ``skew_diagnostics`` (one entry
    per tolerated pair) so the report's non-content section can
    surface what was tolerated. Returns True when the structures
    match, applying the tolerance where possible.
    """

    if left_facets == right_facets:
        return True
    left_ids = set(left_facets)
    right_ids = set(right_facets)
    if left_ids != right_ids:
        return False
    for facet_id in sorted(left_ids):
        left_facet = left_facets[facet_id]
        right_facet = right_facets[facet_id]
        if not isinstance(left_facet, dict) or not isinstance(right_facet, dict):
            if left_facet != right_facet:
                return False
            continue
        if not _facet_equal_with_info_skew(
            facet_id, left_facet, right_facet, skew_diagnostics
        ):
            return False
    return True


def _facet_equal_with_info_skew(
    facet_id: Any,
    left_facet: dict[str, Any],
    right_facet: dict[str, Any],
    skew_diagnostics: list[dict[str, Any]],
) -> bool:
    if left_facet == right_facet:
        return True
    left_meta = {
        key: value
        for key, value in left_facet.items()
        if key != "options"
    }
    right_meta = {
        key: value
        for key, value in right_facet.items()
        if key != "options"
    }
    if left_meta != right_meta:
        return False
    left_options = left_facet.get("options", {})
    right_options = right_facet.get("options", {})
    if not isinstance(left_options, dict) or not isinstance(right_options, dict):
        return False
    if set(left_options) != set(right_options):
        return False
    for option_id in sorted(left_options):
        left_option = left_options[option_id]
        right_option = right_options[option_id]
        if not _options_equal_with_info_skew(
            facet_id, option_id, left_option, right_option, skew_diagnostics
        ):
            return False
    return True


def _options_equal_with_info_skew(
    facet_id: Any,
    option_id: Any,
    left_option: Any,
    right_option: Any,
    skew_diagnostics: list[dict[str, Any]],
) -> bool:
    if left_option == right_option:
        return True
    info_pair = _matching_option_info_pair(left_option, right_option)
    if info_pair is None:
        return False
    left_info, right_info = info_pair
    equal, diagnostic = source_info_equal_with_skew(left_info, right_info)
    if not equal:
        return False
    if diagnostic is not None:
        skew_diagnostics.append(
            {
                "facet_id": facet_id,
                "option_id": option_id,
                "left_info": left_info,
                "right_info": right_info,
                **diagnostic,
            }
        )
    return True


def _matching_option_info_pair(left_option: Any, right_option: Any) -> tuple[str, str] | None:
    if not isinstance(left_option, dict) or not isinstance(right_option, dict):
        return None
    left_info = left_option.get("info")
    right_info = right_option.get("info")
    if not (isinstance(left_info, str) and isinstance(right_info, str)):
        return None
    left_other = {key: value for key, value in left_option.items() if key != "info"}
    right_other = {key: value for key, value in right_option.items() if key != "info"}
    if left_other != right_other:
        return None
    return left_info, right_info


def _strip_required_params_option_info(
    required_params: Any,
) -> Any:
    """Return a rebuilt copy of ``required_params`` with each option's
    ``info`` string removed. The shape, ordering, and non-info fields
    are preserved; only the live-volatile ``info`` key disappears from
    option dicts so a strict top-level comparison can treat the
    remaining content as non-volatile. Structure that is not the
    documented list-of-params-of-options-of-info shape is returned
    unchanged."""

    if not isinstance(required_params, list):
        return required_params
    rebuilt_list: list[Any] = []
    for param in required_params:
        if not isinstance(param, dict):
            rebuilt_list.append(param)
            continue
        rebuilt_param: dict[str, Any] = {}
        for key, value in param.items():
            if key != "options" or not isinstance(value, list):
                rebuilt_param[key] = value
                continue
            rebuilt_options: list[Any] = []
            for option in value:
                if not isinstance(option, dict):
                    rebuilt_options.append(option)
                    continue
                rebuilt_options.append(
                    {opt_key: opt_value for opt_key, opt_value in option.items() if opt_key != "info"}
                )
            rebuilt_param[key] = rebuilt_options
        rebuilt_list.append(rebuilt_param)
    return rebuilt_list


def _required_params_option_info_pairs(
    left: Any, right: Any
) -> list[tuple[str, str, str, str, int, int]]:
    """Pair source-option ``info`` strings from left/right
    ``required_params`` for skew comparison. Pairs are keyed by
    matching ``(param_index, option_id)``; structural mismatches that
    would block pairing are left to the strict top-level equality so
    the existing diff wording still surfaces them."""

    pairs: list[tuple[str, str, str, str, int, int]] = []
    if not isinstance(left, list) or not isinstance(right, list):
        return pairs
    for param_index, (left_param, right_param) in enumerate(zip(left, right)):
        pairs.extend(_required_param_option_info_pairs(param_index, left_param, right_param))
    return pairs


def _required_param_option_info_pairs(
    param_index: int, left_param: Any, right_param: Any
) -> list[tuple[str, str, str, str, int, int]]:
    if not isinstance(left_param, dict) or not isinstance(right_param, dict):
        return []
    left_options = left_param.get("options")
    right_options = right_param.get("options")
    if not isinstance(left_options, list) or not isinstance(right_options, list):
        return []
    right_by_id = {
        option.get("id"): option
        for option in right_options
        if isinstance(option, dict) and isinstance(option.get("id"), str)
    }
    return [
        pair
        for left_index, left_option in enumerate(left_options)
        for pair in [_required_param_option_info_pair(param_index, left_index, left_option, right_by_id)]
        if pair is not None
    ]


def _required_param_option_info_pair(
    param_index: int, left_index: int, left_option: Any, right_by_id: dict[Any, Any]
) -> tuple[str, str, str, str, int, int] | None:
    if not isinstance(left_option, dict):
        return None
    left_id = left_option.get("id")
    left_info = left_option.get("info")
    right_option = right_by_id.get(left_id)
    if not (isinstance(left_id, str) and isinstance(left_info, str) and isinstance(right_option, dict)):
        return None
    right_info = right_option.get("info")
    if not isinstance(right_info, str):
        return None
    path = f"$.required_params[{param_index}].options[{left_index}]"
    return path, left_id, left_info, right_info, param_index, left_index


def required_params_info_skew_result(
    left_required_params: Any, right_required_params: Any
) -> dict[str, Any]:
    """Compare ``required_params`` source-option ``info`` strings with
    the bounded skew tolerance.

    Returns a dict with:

    - ``stripped_left`` / ``stripped_right``: rebuilt copies of the
      inputs with each option's ``info`` field removed (other fields
      intact, suitable for strict top-level comparison).
    - ``equal``: True iff every paired option matched under the
      tolerance. Structural non-info differences between the original
      inputs are not detected here; they remain in the strict
      top-level comparison.
    - ``tolerances``: list of skew entries (one per tolerated pair).
      Each entry carries a ``path`` (e.g.
      ``$.required_params[0].options[0]``), a ``source`` of
      ``"required_params"``, the option id, the two raw ``info``
      strings, and the standard skew-bounds payload.
    """

    stripped_left = _strip_required_params_option_info(left_required_params)
    stripped_right = _strip_required_params_option_info(right_required_params)
    pairs = _required_params_option_info_pairs(
        left_required_params, right_required_params
    )
    tolerances: list[dict[str, Any]] = []
    for path, option_id, left_info, right_info, _param_index, _option_index in pairs:
        if left_info == right_info:
            continue
        equal, diagnostic = source_info_equal_with_skew(left_info, right_info)
        if not equal:
            return {
                "stripped_left": stripped_left,
                "stripped_right": stripped_right,
                "equal": False,
                "tolerances": tolerances,
            }
        if diagnostic is not None:
            tolerances.append(
                {
                    "path": path,
                    "source": "required_params",
                    "option_id": option_id,
                    "left_info": left_info,
                    "right_info": right_info,
                    **diagnostic,
                }
            )
    return {
        "stripped_left": stripped_left,
        "stripped_right": stripped_right,
        "equal": True,
        "tolerances": tolerances,
    }


def _strip_request_window_echoes(request: Any) -> Any:
    """Return a copy of ``request`` with the parse-time-anchored
    ``after`` / ``before`` echoes removed so a strict top-level
    comparison treats them as non-volatile. Other request fields
    remain content. Structure that is not the documented ``_request``
    dict shape is returned unchanged."""

    if not isinstance(request, dict):
        return request
    rebuilt: dict[str, Any] = {}
    for key, value in request.items():
        if key in ("after", "before"):
            continue
        rebuilt[key] = value
    return rebuilt


def request_window_skew_result(
    left_request: Any, right_request: Any
) -> dict[str, Any]:
    """SOW-0104 fix-10: compare the ``_request.after`` /
    ``_request.before`` echoes with a bounded skew tolerance.

    The echoes embed parse-time ``unix_now_seconds()`` by reference
    design (Rust L1418 -> L3624-3690). Two peers invoked seconds
    apart legitimately produce different echoes; a slow third
    peer must not surface as a false-positive content mismatch.
    The tolerance mirrors the fix-4 source-info tolerance
    precedent (same 300s bound, same diagnostics style).

    Returns a dict with:

    - ``stripped_left`` / ``stripped_right``: rebuilt copies of the
      inputs with ``after`` / ``before`` removed (other fields
      intact, suitable for strict top-level comparison).
    - ``equal``: True iff every paired echo matched under the
      tolerance, OR the echoes are missing from one or both
      requests. Structural non-echo differences between the
      original inputs are not detected here; they remain in the
      strict top-level comparison.
    - ``tolerances``: list of skew entries (one per tolerated
      field). Each entry carries the field name, the two raw
      echo values, the delta in seconds, and the bound.
    """

    stripped_left = _strip_request_window_echoes(left_request)
    stripped_right = _strip_request_window_echoes(right_request)
    if not isinstance(left_request, dict) or not isinstance(right_request, dict):
        return {
            "stripped_left": stripped_left,
            "stripped_right": stripped_right,
            "equal": True,
            "tolerances": [],
        }
    tolerances: list[dict[str, Any]] = []
    for field in ("after", "before"):
        if field not in left_request or field not in right_request:
            continue
        left_value = left_request[field]
        right_value = right_request[field]
        if not isinstance(left_value, int) or not isinstance(right_value, int):
            # Non-integer echoes are out of the documented shape; let
            # the strict top-level comparison handle them.
            continue
        if left_value == right_value:
            continue
        delta = abs(int(left_value) - int(right_value))
        if delta > REQUEST_WINDOW_SKEW_TOLERANCE_SECONDS:
            # Skew beyond the bound: the strict comparison stays and
            # the original first_difference surfaces the field path
            # and both values.
            return {
                "stripped_left": stripped_left,
                "stripped_right": stripped_right,
                "equal": False,
                "tolerances": tolerances,
            }
        tolerances.append(
            {
                "field": field,
                "left_seconds": int(left_value),
                "right_seconds": int(right_value),
                "delta_seconds": delta,
                "bound_seconds": REQUEST_WINDOW_SKEW_TOLERANCE_SECONDS,
            }
        )
    return {
        "stripped_left": stripped_left,
        "stripped_right": stripped_right,
        "equal": True,
        "tolerances": tolerances,
    }


def selected_facet_options_removed(
    facets: dict[str, Any], ignored_facet_ids: set[str]
) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for facet_id, facet in facets.items():
        if facet_id not in ignored_facet_ids or not isinstance(facet, dict):
            out[facet_id] = facet
            continue
        normalized = dict(facet)
        # The installed plugin's selected+faceted behavior affects option
        # counts for the selected facet. Facet identity/metadata remains content.
        normalized["options"] = {}
        out[facet_id] = normalize_json(normalized)
    return out


def non_content_facet_artifacts(doc: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    facets = doc.get("facets")
    if not isinstance(facets, list):
        facets = doc.get("facets_delta")
    if not isinstance(facets, list):
        return {}
    artifacts: dict[str, list[dict[str, Any]]] = {}
    for facet in facets:
        if not isinstance(facet, dict):
            continue
        facet_id = facet.get("id")
        if not isinstance(facet_id, str):
            continue
        for option in facet.get("options", []):
            if not isinstance(option, dict):
                continue
            if (
                option.get("id") == NETDATA_EMPTY_STRING_FACET_HASH_ID
                and option.get("name") == NETDATA_UNAVAILABLE_FIELD_LABEL
                and option.get("count") == 0
            ):
                artifacts.setdefault(facet_id, []).append(normalize_json(option))
    return artifacts


def raw_histogram(doc: dict[str, Any]) -> Any:
    histogram = doc.get("histogram")
    if not isinstance(histogram, dict):
        histogram = doc.get("histogram_delta")
    return histogram


def normalized_histogram_result(histogram: dict[str, Any]) -> tuple[list[str], list[Any]] | None:
    result = histogram.get("chart", {}).get("result")
    if not isinstance(result, dict):
        return None
    labels = result.get("labels")
    data = result.get("data")
    if not isinstance(labels, list) or not labels or not isinstance(data, list):
        return None
    dimensions = [label for label in labels[1:] if isinstance(label, str)]
    return dimensions, data


def normalized_histogram_buckets(dimensions: list[str], data: list[Any]) -> dict[str, dict[str, Any]]:
    buckets: dict[str, dict[str, Any]] = {}
    for row in data:
        if not isinstance(row, list) or not row:
            continue
        timestamp = row[0]
        values: dict[str, Any] = {}
        for index, dimension in enumerate(dimensions, start=1):
            point = row[index] if index < len(row) else None
            values[dimension] = normalize_json(point)
        buckets[str(timestamp)] = values
    return buckets


def normalized_histogram(doc: dict[str, Any]) -> Any:
    histogram = raw_histogram(doc)
    if not isinstance(histogram, dict):
        return normalize_json(histogram)
    histogram_result = normalized_histogram_result(histogram)
    if histogram_result is None:
        return normalize_json(histogram)
    dimensions, data = histogram_result
    return normalize_json(
        {
            "id": histogram.get("id"),
            "name": histogram.get("name"),
            "buckets": normalized_histogram_buckets(dimensions, data),
        }
    )


def histogram_chart_schema_errors(doc: dict[str, Any]) -> list[str]:
    histogram = raw_histogram(doc)
    if not isinstance(histogram, dict):
        return []
    chart = histogram.get("chart")
    if not isinstance(chart, dict):
        return ["histogram.chart"]
    errors: list[str] = []
    required_chart_objects = ("summary", "totals", "result", "db", "view")
    for key in required_chart_objects:
        if not isinstance(chart.get(key), dict):
            errors.append(f"histogram.chart.{key}")
    if not isinstance(chart.get("agents"), list):
        errors.append("histogram.chart.agents")
    errors.extend(histogram_result_schema_errors(chart.get("result")))
    errors.extend(histogram_db_schema_errors(chart.get("db")))
    errors.extend(histogram_view_schema_errors(chart.get("view")))
    return errors


def histogram_result_schema_errors(result: Any) -> list[str]:
    errors: list[str] = []
    if isinstance(result, dict):
        if not isinstance(result.get("labels"), list):
            errors.append("histogram.chart.result.labels")
        if not isinstance(result.get("point"), dict):
            errors.append("histogram.chart.result.point")
        if not isinstance(result.get("data"), list):
            errors.append("histogram.chart.result.data")
    return errors


def histogram_db_schema_errors(db: Any) -> list[str]:
    errors: list[str] = []
    if isinstance(db, dict):
        db_dimensions = db.get("dimensions")
        if not isinstance(db_dimensions, dict):
            errors.append("histogram.chart.db.dimensions")
        else:
            for key in ("ids", "names", "units"):
                if not isinstance(db_dimensions.get(key), list):
                    errors.append(f"histogram.chart.db.dimensions.{key}")
            append_histogram_sts_schema_errors(
                errors,
                "histogram.chart.db.dimensions.sts",
                db_dimensions.get("sts"),
            )
        if not isinstance(db.get("per_tier"), list):
            errors.append("histogram.chart.db.per_tier")
    return errors


def histogram_view_schema_errors(view: Any) -> list[str]:
    errors: list[str] = []
    if isinstance(view, dict):
        view_dimensions = view.get("dimensions")
        if not isinstance(view_dimensions, dict):
            errors.append("histogram.chart.view.dimensions")
        else:
            for key in ("grouped_by", "ids", "names", "colors", "units"):
                if not isinstance(view_dimensions.get(key), list):
                    errors.append(f"histogram.chart.view.dimensions.{key}")
            append_histogram_sts_schema_errors(
                errors,
                "histogram.chart.view.dimensions.sts",
                view_dimensions.get("sts"),
            )
    return errors


def append_histogram_sts_schema_errors(
    errors: list[str], prefix: str, value: Any
) -> None:
    if not isinstance(value, dict):
        errors.append(prefix)
        return
    for key in ("min", "max", "avg", "arp", "con"):
        if not isinstance(value.get(key), list):
            errors.append(f"{prefix}.{key}")


def normalized_items(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items")
    if not isinstance(items, dict):
        items = doc.get("items_delta")
    if not isinstance(items, dict):
        return {}
    return normalize_json(
        {key: value for key, value in items.items() if key not in DIAGNOSTIC_ITEM_FIELDS}
    )


def normalized_diagnostic_items(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items")
    if not isinstance(items, dict):
        items = doc.get("items_delta")
    if not isinstance(items, dict):
        return {}
    return normalize_json(
        {key: value for key, value in items.items() if key in DIAGNOSTIC_ITEM_FIELDS}
    )


def normalized_top_level(doc: dict[str, Any]) -> dict[str, Any]:
    normalized = {
        key: normalize_json(value)
        for key, value in doc.items()
        if key
        not in {
            *VOLATILE_TOP_LEVEL_FIELDS,
            "columns",
            "data",
            "facets",
            "facets_delta",
            "histogram",
            "histogram_delta",
            "items",
            "items_delta",
        }
    }
    request = normalized.get("_request")
    if isinstance(request, dict) and request.get("info") is True:
        request = dict(request)
        request.pop("after", None)
        request.pop("before", None)
        normalized["_request"] = normalize_json(request)
    return normalized


def is_function_error(doc: dict[str, Any]) -> bool:
    return isinstance(doc.get("errorMessage"), str)


def ignored_data_only_columns(left: dict[str, Any], right: dict[str, Any]) -> dict[str, list[str]]:
    if not (is_data_only(left) or is_data_only(right)):
        return {"left": [], "right": []}
    left_ignored = set(column_indices(left)) - columns_with_any_returned_value(left)
    right_ignored = set(column_indices(right)) - columns_with_any_returned_value(right)
    return {
        "left": sorted(left_ignored - columns_with_any_returned_value(right)),
        "right": sorted(right_ignored - columns_with_any_returned_value(left)),
    }


def normalize_json(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: normalize_json(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return [normalize_json(item) for item in value]
    return value


def first_difference(left: Any, right: Any, path: str = "$") -> str | None:
    if type(left) is not type(right):
        return f"{path}: type differs ({type(left).__name__} != {type(right).__name__})"
    if isinstance(left, dict):
        left_keys = set(left)
        right_keys = set(right)
        only_left = sorted(left_keys - right_keys)
        only_right = sorted(right_keys - left_keys)
        if only_left:
            return f"{path}: key only on left: {only_left[0]!r}"
        if only_right:
            return f"{path}: key only on right: {only_right[0]!r}"
        for key in sorted(left_keys):
            diff = first_difference(left[key], right[key], f"{path}.{key}")
            if diff:
                return diff
        return None
    if isinstance(left, list):
        if len(left) != len(right):
            return f"{path}: list length differs ({len(left)} != {len(right)})"
        for index, (left_item, right_item) in enumerate(zip(left, right)):
            diff = first_difference(left_item, right_item, f"{path}[{index}]")
            if diff:
                return diff
        return None
    if left != right:
        return f"{path}: value differs ({left!r} != {right!r})"
    return None


def value_count(facets: Any) -> int:
    if isinstance(facets, dict):
        return sum(
            len(facet.get("options", {}))
            for facet in facets.values()
            if isinstance(facet, dict)
        )
    return 0


def _compare_function_error(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    left_error = normalize_json(left)
    right_error = normalize_json(right)
    ok = left_error == right_error
    return {
        "ok": ok,
        "checks": {"function_error": ok},
        "content_checks": {"function_error": ok},
        "diffs": {"function_error": first_difference(left_error, right_error)},
        "non_content": {},
        "left": {
            "top_level_keys": sorted(left),
            "status": left.get("status"),
            "errorMessage": left.get("errorMessage"),
        },
        "right": {
            "top_level_keys": sorted(right),
            "status": right.get("status"),
            "errorMessage": right.get("errorMessage"),
        },
        "ignored_top_level_fields": sorted(VOLATILE_TOP_LEVEL_FIELDS),
    }


def _normalized_comparison_parts(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    data_only = is_data_only(left) or is_data_only(right)
    allowed_columns = None
    if data_only:
        allowed_columns = columns_with_any_returned_value(left) | columns_with_any_returned_value(right)
    return {
        "left_columns": normalized_columns(left, allowed_columns),
        "right_columns": normalized_columns(right, allowed_columns),
        "left_rows": normalized_rows(left, allowed_columns),
        "right_rows": normalized_rows(right, allowed_columns),
        "left_facets": normalized_facets(left),
        "right_facets": normalized_facets(right),
        "left_histogram": normalized_histogram(left),
        "right_histogram": normalized_histogram(right),
        "left_histogram_schema_errors": histogram_chart_schema_errors(left),
        "right_histogram_schema_errors": histogram_chart_schema_errors(right),
        "left_items": normalized_items(left),
        "right_items": normalized_items(right),
        "left_diagnostic_items": normalized_diagnostic_items(left),
        "right_diagnostic_items": normalized_diagnostic_items(right),
        "left_top_level": normalized_top_level(left),
        "right_top_level": normalized_top_level(right),
    }


def _top_level_check(parts: dict[str, Any]) -> tuple[bool, dict[str, Any], dict[str, Any]]:
    required_params_skew = required_params_info_skew_result(
        parts["left_top_level"].get("required_params"),
        parts["right_top_level"].get("required_params"),
    )
    request_window_skew = request_window_skew_result(
        parts["left_top_level"].get("_request"),
        parts["right_top_level"].get("_request"),
    )
    left_top_level_for_strict = dict(parts["left_top_level"])
    right_top_level_for_strict = dict(parts["right_top_level"])
    left_top_level_for_strict["required_params"] = required_params_skew["stripped_left"]
    right_top_level_for_strict["required_params"] = required_params_skew["stripped_right"]
    if isinstance(left_top_level_for_strict.get("_request"), dict) and isinstance(
        right_top_level_for_strict.get("_request"), dict
    ):
        left_top_level_for_strict["_request"] = request_window_skew["stripped_left"]
        right_top_level_for_strict["_request"] = request_window_skew["stripped_right"]
    return (
        left_top_level_for_strict == right_top_level_for_strict
        and required_params_skew["equal"]
        and request_window_skew["equal"],
        required_params_skew,
        request_window_skew,
    )


def _comparison_diffs(parts: dict[str, Any]) -> dict[str, Any]:
    left_schema = parts["left_histogram_schema_errors"]
    right_schema = parts["right_histogram_schema_errors"]
    return {
        "top_level": first_difference(parts["left_top_level"], parts["right_top_level"]),
        "columns": first_difference(parts["left_columns"], parts["right_columns"]),
        "rows": first_difference(parts["left_rows"], parts["right_rows"]),
        "facets": first_difference(parts["left_facets"], parts["right_facets"]),
        "histogram": first_difference(parts["left_histogram"], parts["right_histogram"]),
        "histogram_schema": {"left": left_schema, "right": right_schema}
        if left_schema or right_schema else None,
        "items": first_difference(parts["left_items"], parts["right_items"]),
        "diagnostic_items": first_difference(parts["left_diagnostic_items"], parts["right_diagnostic_items"]),
    }


def _side_summary(top_level: dict[str, Any], columns: Any, rows: Any, facets: Any, items: Any) -> dict[str, Any]:
    return {
        "top_level_keys": sorted(top_level),
        "columns": len(columns),
        "rows": len(rows),
        "facets": len(facets),
        "facet_values": value_count(facets),
        "items": items,
    }


def compare(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    if is_function_error(left) or is_function_error(right):
        return _compare_function_error(left, right)

    parts = _normalized_comparison_parts(left, right)
    rows_equal, ignored_message_rows = rows_match_with_known_plugin_message_corruption(
        parts["left_rows"], parts["right_rows"]
    )
    (
        facets_equal,
        ignored_selected_facets,
        info_skew_tolerances,
    ) = facets_match_with_selected_field_quirk(
        left, right, parts["left_facets"], parts["right_facets"]
    )
    top_level_equal, required_params_skew, request_window_skew = _top_level_check(parts)
    checks = {
        "top_level": top_level_equal,
        "columns": parts["left_columns"] == parts["right_columns"],
        "rows": rows_equal,
        "facets": facets_equal,
        "histogram": parts["left_histogram"] == parts["right_histogram"],
        "histogram_schema": not parts["left_histogram_schema_errors"]
        and not parts["right_histogram_schema_errors"],
        "items": parts["left_items"] == parts["right_items"],
        "diagnostic_items": parts["left_diagnostic_items"] == parts["right_diagnostic_items"],
    }
    content_checks = {
        key: value for key, value in checks.items() if key != "diagnostic_items"
    }
    return {
        "ok": all(content_checks.values()),
        "checks": checks,
        "content_checks": content_checks,
        "diffs": _comparison_diffs(parts),
        "non_content": {
            "left_empty_unavailable_facet_artifacts": non_content_facet_artifacts(left),
            "right_empty_unavailable_facet_artifacts": non_content_facet_artifacts(right),
            "data_only_ignored_all_null_columns": ignored_data_only_columns(left, right),
            "known_plugin_message_corruption_rows": ignored_message_rows,
            "selected_field_facet_quirks": ignored_selected_facets,
            "source_option_info_skew_tolerances": info_skew_tolerances,
            "required_params_source_info_skew_tolerances": required_params_skew["tolerances"],
            "request_window_skew_tolerances": request_window_skew["tolerances"],
        },
        "left": _side_summary(parts["left_top_level"], parts["left_columns"], parts["left_rows"],
                              parts["left_facets"], parts["left_items"]),
        "right": _side_summary(parts["right_top_level"], parts["right_columns"], parts["right_rows"],
                               parts["right_facets"], parts["right_items"]),
        "ignored_top_level_fields": sorted(VOLATILE_TOP_LEVEL_FIELDS),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()

    report = compare(load_json(args.left), load_json(args.right))
    output = json.dumps(report, indent=2, sort_keys=True)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(output + "\n", encoding="utf-8")
    else:
        print(output)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
