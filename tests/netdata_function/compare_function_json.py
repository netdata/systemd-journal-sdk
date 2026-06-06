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
reported, but they do not decide content equality.
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


def column_indices(doc: dict[str, Any]) -> dict[str, int]:
    columns = doc.get("columns")
    if not isinstance(columns, dict):
        return {}
    out: dict[str, int] = {}
    for name, meta in columns.items():
        if isinstance(meta, dict) and isinstance(meta.get("index"), int):
            out[name] = meta["index"]
    return out


def normalized_columns(doc: dict[str, Any]) -> dict[str, dict[str, Any]]:
    columns = doc.get("columns")
    if not isinstance(columns, dict):
        return {}
    out: dict[str, dict[str, Any]] = {}
    for name, meta in columns.items():
        if not isinstance(name, str) or not isinstance(meta, dict):
            continue
        normalized = dict(meta)
        for field in VOLATILE_COLUMN_METADATA_FIELDS:
            normalized.pop(field, None)
        out[name] = normalize_json(normalized)
    return out


def normalized_rows(doc: dict[str, Any]) -> list[dict[str, Any]]:
    indices = column_indices(doc)
    rows = doc.get("data")
    if not isinstance(rows, list):
        return []
    ordered_columns = sorted(indices, key=lambda name: indices[name])
    out: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, list):
            continue
        item: dict[str, Any] = {}
        for field in ordered_columns:
            idx = indices.get(field)
            if idx is not None and 0 <= idx < len(row):
                item[field] = normalize_json(row[idx])
            else:
                item[field] = None
        out.append(item)
    return out


def normalized_facets(doc: dict[str, Any]) -> dict[str, Any]:
    facets = doc.get("facets")
    if not isinstance(facets, list):
        return {}
    out: dict[str, Any] = {}
    for facet in facets:
        if not isinstance(facet, dict):
            continue
        facet_id = facet.get("id")
        if not isinstance(facet_id, str):
            continue
        normalized = {
            key: value
            for key, value in facet.items()
            if key not in {*VOLATILE_FACET_METADATA_FIELDS, "options"}
        }
        options: dict[str, Any] = {}
        for option in facet.get("options", []):
            if not isinstance(option, dict):
                continue
            option_id = option.get("id")
            if not isinstance(option_id, str):
                continue
            if (
                option_id == NETDATA_EMPTY_STRING_FACET_HASH_ID
                and option.get("name") == NETDATA_UNAVAILABLE_FIELD_LABEL
                and option.get("count") == 0
            ):
                continue
            options[option_id] = normalize_json(
                {
                    key: value
                    for key, value in option.items()
                    if key not in VOLATILE_FACET_METADATA_FIELDS
                }
            )
        normalized["options"] = options
        out[facet_id] = normalize_json(normalized)
    return out


def non_content_facet_artifacts(doc: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    facets = doc.get("facets")
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


def normalized_histogram(doc: dict[str, Any]) -> Any:
    histogram = doc.get("histogram")
    if not isinstance(histogram, dict):
        return normalize_json(histogram)
    result = histogram.get("chart", {}).get("result")
    if not isinstance(result, dict):
        return normalize_json(histogram)
    labels = result.get("labels")
    data = result.get("data")
    if not isinstance(labels, list) or not labels or not isinstance(data, list):
        return normalize_json(histogram)
    dimensions = [label for label in labels[1:] if isinstance(label, str)]
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
    return normalize_json(
        {
            "id": histogram.get("id"),
            "name": histogram.get("name"),
            "buckets": buckets,
        }
    )


def normalized_items(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items")
    if not isinstance(items, dict):
        return {}
    return normalize_json(
        {key: value for key, value in items.items() if key not in DIAGNOSTIC_ITEM_FIELDS}
    )


def normalized_diagnostic_items(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items")
    if not isinstance(items, dict):
        return {}
    return normalize_json(
        {key: value for key, value in items.items() if key in DIAGNOSTIC_ITEM_FIELDS}
    )


def normalized_top_level(doc: dict[str, Any]) -> dict[str, Any]:
    return {
        key: normalize_json(value)
        for key, value in doc.items()
        if key
        not in {
            *VOLATILE_TOP_LEVEL_FIELDS,
            "columns",
            "data",
            "facets",
            "histogram",
            "items",
        }
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
    left_columns = normalized_columns(left)
    right_columns = normalized_columns(right)
    left_rows = normalized_rows(left)
    right_rows = normalized_rows(right)
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
    checks = {
        "top_level": left_top_level == right_top_level,
        "columns": left_columns == right_columns,
        "rows": left_rows == right_rows,
        "facets": left_facets == right_facets,
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
