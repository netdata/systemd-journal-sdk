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
) -> tuple[bool, list[str]]:
    if left_facets == right_facets:
        return True, []
    ignored = sorted(request_selected_facet_ids(left, right))
    if not ignored:
        return False, []
    ignored_set = set(ignored)
    left_filtered = selected_facet_options_removed(left_facets, ignored_set)
    right_filtered = selected_facet_options_removed(right_facets, ignored_set)
    return left_filtered == right_filtered, ignored


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


def compare(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    if is_function_error(left) or is_function_error(right):
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

    data_only = is_data_only(left) or is_data_only(right)
    allowed_columns = None
    if data_only:
        allowed_columns = columns_with_any_returned_value(
            left
        ) | columns_with_any_returned_value(right)
    left_columns = normalized_columns(left, allowed_columns)
    right_columns = normalized_columns(right, allowed_columns)
    left_rows = normalized_rows(left, allowed_columns)
    right_rows = normalized_rows(right, allowed_columns)
    left_facets = normalized_facets(left)
    right_facets = normalized_facets(right)
    left_histogram = normalized_histogram(left)
    right_histogram = normalized_histogram(right)
    left_items = normalized_items(left)
    right_items = normalized_items(right)
    left_diagnostic_items = normalized_diagnostic_items(left)
    right_diagnostic_items = normalized_diagnostic_items(right)
    left_top_level = normalized_top_level(left)
    right_top_level = normalized_top_level(right)
    rows_equal, ignored_message_rows = rows_match_with_known_plugin_message_corruption(
        left_rows, right_rows
    )
    facets_equal, ignored_selected_facets = facets_match_with_selected_field_quirk(
        left, right, left_facets, right_facets
    )
    checks = {
        "top_level": left_top_level == right_top_level,
        "columns": left_columns == right_columns,
        "rows": rows_equal,
        "facets": facets_equal,
        "histogram": left_histogram == right_histogram,
        "items": left_items == right_items,
        "diagnostic_items": left_diagnostic_items == right_diagnostic_items,
    }
    content_checks = {
        key: value for key, value in checks.items() if key != "diagnostic_items"
    }
    return {
        "ok": all(content_checks.values()),
        "checks": checks,
        "content_checks": content_checks,
        "diffs": {
            "top_level": first_difference(left_top_level, right_top_level),
            "columns": first_difference(left_columns, right_columns),
            "rows": first_difference(left_rows, right_rows),
            "facets": first_difference(left_facets, right_facets),
            "histogram": first_difference(left_histogram, right_histogram),
            "items": first_difference(left_items, right_items),
            "diagnostic_items": first_difference(left_diagnostic_items, right_diagnostic_items),
        },
        "non_content": {
            "left_empty_unavailable_facet_artifacts": non_content_facet_artifacts(left),
            "right_empty_unavailable_facet_artifacts": non_content_facet_artifacts(right),
            "data_only_ignored_all_null_columns": ignored_data_only_columns(left, right),
            "known_plugin_message_corruption_rows": ignored_message_rows,
            "selected_field_facet_quirks": ignored_selected_facets,
        },
        "left": {
            "top_level_keys": sorted(left_top_level),
            "columns": len(left_columns),
            "rows": len(left_rows),
            "facets": len(left_facets),
            "facet_values": value_count(left_facets),
            "items": left_items,
        },
        "right": {
            "top_level_keys": sorted(right_top_level),
            "columns": len(right_columns),
            "rows": len(right_rows),
            "facets": len(right_facets),
            "facet_values": value_count(right_facets),
            "items": right_items,
        },
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
