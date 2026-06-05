#!/usr/bin/env python3
"""Compare Netdata function JSON at the semantic boundary.

The comparison deliberately ignores volatile envelope fields and focuses on the
contract that matters for SDK/plugin equivalence: status, returned rows,
nonzero facet counters, nonzero histogram totals, and item counters that are
stable across implementations. Zero-count vocabulary padding is intentionally
ignored because the plugin may preserve values discovered while scanning rows
that do not contribute to the result.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROW_FIELDS = (
    "timestamp",
    "rowOptions",
    "PRIORITY",
    "_HOSTNAME",
    "ND_JOURNAL_PROCESS",
    "MESSAGE",
    "SYSLOG_FACILITY",
    "ERRNO",
)


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


def normalized_rows(doc: dict[str, Any]) -> list[dict[str, Any]]:
    indices = column_indices(doc)
    rows = doc.get("data")
    if not isinstance(rows, list):
        return []
    out: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, list):
            continue
        item: dict[str, Any] = {}
        for field in ROW_FIELDS:
            idx = indices.get(field)
            if idx is not None and 0 <= idx < len(row):
                item[field] = row[idx]
        out.append(item)
    return out


def normalized_facets(doc: dict[str, Any]) -> dict[str, dict[str, int]]:
    facets = doc.get("facets")
    if not isinstance(facets, list):
        return {}
    out: dict[str, dict[str, int]] = {}
    for facet in facets:
        if not isinstance(facet, dict):
            continue
        facet_id = facet.get("id")
        if not isinstance(facet_id, str):
            continue
        values: dict[str, int] = {}
        for option in facet.get("options", []):
            if not isinstance(option, dict):
                continue
            option_id = option.get("id")
            count = option.get("count")
            if isinstance(option_id, str) and isinstance(count, int) and count != 0:
                values[option_id] = count
        if values:
            out[facet_id] = values
    return out


def histogram_totals(doc: dict[str, Any]) -> dict[str, int]:
    result = (
        doc.get("histogram", {})
        .get("chart", {})
        .get("result", {})
        if isinstance(doc.get("histogram"), dict)
        else {}
    )
    if not isinstance(result, dict):
        return {}
    labels = result.get("labels")
    data = result.get("data")
    if not isinstance(labels, list) or not labels or not isinstance(data, list):
        return {}
    dimension_labels = [label for label in labels[1:] if isinstance(label, str)]
    totals = {label: 0 for label in dimension_labels}
    for row in data:
        if not isinstance(row, list):
            continue
        for label, point in zip(dimension_labels, row[1:]):
            if isinstance(point, list) and point and isinstance(point[0], int):
                totals[label] += point[0]
            elif isinstance(point, int):
                totals[label] += point
    return {label: count for label, count in totals.items() if count != 0}


def stable_items(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items")
    if not isinstance(items, dict):
        return {}
    return {
        key: items.get(key)
        for key in ("matched", "returned", "max_to_return")
        if key in items
    }


def compare(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    checks = {
        "status": left.get("status") == right.get("status"),
        "rows": normalized_rows(left) == normalized_rows(right),
        "facets": normalized_facets(left) == normalized_facets(right),
        "histogram_totals": histogram_totals(left) == histogram_totals(right),
        "items": stable_items(left) == stable_items(right),
    }
    return {
        "ok": all(checks.values()),
        "checks": checks,
        "left": {
            "rows": len(normalized_rows(left)),
            "facets": sorted(normalized_facets(left)),
            "histogram_totals": histogram_totals(left),
            "items": stable_items(left),
        },
        "right": {
            "rows": len(normalized_rows(right)),
            "facets": sorted(normalized_facets(right)),
            "histogram_totals": histogram_totals(right),
            "items": stable_items(right),
        },
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
