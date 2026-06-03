#!/usr/bin/env python3
"""Render standard benchmark reports from benchmark JSON artifacts."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


LANGUAGE_ORDER = ["systemd", "rust", "go", "node", "python"]
SURFACE_ORDER = ["file", "open-files", "directory", "writer-core"]

READER_PRODUCTION_ORDER = [
    ("systemd", "data", "", ""),
    ("rust", "core-payloads", "live", "windowed"),
    ("rust", "sdk-payloads", "live", "windowed"),
    ("rust", "facade-data", "live", "windowed"),
    ("go", "sdk-payloads", "live", "mmap"),
    ("go", "facade-data", "live", "mmap"),
    ("node", "sdk-payloads", "live", "buffer"),
    ("node", "facade-data", "live", "buffer"),
    ("python", "sdk-payloads", "live", "mmap"),
    ("python", "facade-data", "live", "mmap"),
]

READER_MODE_ORDER = [
    "data",
    "core-next",
    "core-offsets",
    "core-payloads",
    "sdk-entry",
    "sdk-payloads",
    "facade-data",
    "next",
]

CONCLUSION_CHOICES = {
    "clear-win": "clear win",
    "mixed": "mixed",
    "no-measurable-change": "no measurable change",
    "regression": "regression",
    "inconclusive": "inconclusive / noisy",
    "not-assessed": "not assessed",
}


@dataclass(frozen=True)
class BenchmarkRun:
    label: str
    kind: str
    source: Path
    run_dir: Path
    summary_path: Path
    manifest_path: Path | None
    summary: Any
    manifest: dict[str, Any]


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SystemExit(f"missing benchmark artifact: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from exc


def resolve_artifact(path: Path, seen: set[Path] | None = None) -> tuple[Path, Path | None]:
    path = path.expanduser()
    seen = set() if seen is None else seen
    resolved = path.resolve(strict=False)
    if resolved in seen:
        raise SystemExit(f"benchmark artifact path cycle detected at {path}")
    seen.add(resolved)
    if path.is_dir():
        candidate = path / "summary.json"
        if candidate.exists():
            return candidate, path / "manifest.json"
        candidate = path / "report.json"
        if candidate.exists():
            return candidate, None
        latest = path / "latest"
        if latest.exists():
            return resolve_artifact(latest, seen)
        raise SystemExit(f"directory has no summary.json, report.json, or latest symlink: {path}")

    if path.name == "manifest.json":
        return path.with_name("summary.json"), path
    if path.name == "summary.json":
        return path, path.with_name("manifest.json")
    if path.name == "report.json":
        return path, None
    raise SystemExit(f"unsupported benchmark artifact path: {path}")


def load_run(path: Path, label: str) -> BenchmarkRun:
    artifact_path, manifest_path = resolve_artifact(path)
    artifact = load_json(artifact_path)

    if artifact_path.name == "report.json":
        kind, summary, manifest = load_writer_core_run(artifact_path, artifact)
    else:
        kind, summary, manifest = load_reader_core_run(artifact_path, manifest_path, artifact)

    return BenchmarkRun(
        label=label,
        kind=kind,
        source=path,
        run_dir=artifact_path.parent,
        summary_path=artifact_path,
        manifest_path=manifest_path if manifest_path and manifest_path.exists() else None,
        summary=summary,
        manifest=manifest,
    )


def load_writer_core_run(
    artifact_path: Path,
    artifact: Any,
) -> tuple[str, Any, dict[str, Any]]:
    if not isinstance(artifact, dict):
        raise SystemExit(f"writer report must be a JSON object: {artifact_path}")
    benchmark = artifact.get("benchmark")
    if benchmark != "writer-core":
        raise SystemExit(f"unsupported report benchmark {benchmark!r} in {artifact_path}")
    environment = dict_or_empty(artifact.get("environment"))
    parameters = dict_or_empty(artifact.get("parameters"))
    manifest = {
        "created_at": environment.get("timestamp_utc") or artifact_path.parent.name,
        "parameters": parameters,
        "environment": environment,
        "status": artifact.get("status", ""),
    }
    return "writer-core", artifact.get("summary", {}), manifest


def load_reader_core_run(
    artifact_path: Path,
    manifest_path: Path | None,
    artifact: Any,
) -> tuple[str, Any, dict[str, Any]]:
    if not isinstance(artifact, list):
        raise SystemExit(f"reader summary must be a JSON array: {artifact_path}")
    manifest: dict[str, Any] = {}
    if manifest_path is not None and manifest_path.exists():
        manifest = load_json(manifest_path)
        if not isinstance(manifest, dict):
            raise SystemExit(f"manifest must be a JSON object: {manifest_path}")
    return "reader-core", artifact, manifest


def dict_or_empty(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def fmt_int(value: Any) -> str:
    if value is None:
        return "-"
    try:
        return f"{float(value):,.0f}"
    except (TypeError, ValueError):
        return str(value)


def fmt_ratio(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:.3f}x"


def fmt_ratio_to(value: float | None, reference: float | None) -> str:
    if value is None or reference is None:
        return "-"
    if reference == 0:
        return "n/a" if value == 0 else "inf"
    return fmt_ratio(value / reference)


def fmt_delta(before: float | None, after: float | None) -> tuple[str, str]:
    if before is None or after is None or before == 0:
        return "-", "-"
    ratio = after / before
    return f"{(ratio - 1.0) * 100.0:+.1f}%", f"{ratio:.3f}x"


def access_from_parts(bounds: Any, strategy: Any) -> str:
    bounds = "" if bounds is None else str(bounds)
    strategy = "" if strategy is None else str(strategy)
    if bounds and strategy:
        return f"{bounds}/{strategy}"
    if bounds:
        return bounds
    if strategy:
        return strategy
    return "stock"


def access_label(row: dict[str, Any]) -> str:
    return access_from_parts(row.get("bounds", ""), row.get("mmap_strategy", ""))


def join_values(value: Any) -> str:
    if value is None:
        return "-"
    if isinstance(value, (list, tuple)):
        return ",".join(map(str, value)) or "-"
    return str(value)


def required_float(mapping: dict[str, Any], field: str, run: BenchmarkRun, context: str) -> float:
    if field not in mapping:
        raise SystemExit(f"missing {field} in {run.summary_path} ({context})")
    try:
        return float(mapping[field])
    except (TypeError, ValueError) as exc:
        raise SystemExit(
            f"invalid numeric {field} in {run.summary_path} ({context}): {mapping[field]!r}"
        ) from exc


def optional_float(mapping: dict[str, Any], field: str, run: BenchmarkRun, context: str) -> float | None:
    if field not in mapping or mapping[field] is None:
        return None
    return required_float(mapping, field, run, context)


def configured_languages(run: BenchmarkRun) -> set[str]:
    value = run.manifest.get("languages")
    parameters = run.manifest.get("parameters", {})
    if value is None and isinstance(parameters, dict):
        value = parameters.get("languages")
    if isinstance(value, (list, tuple, set)):
        return {str(item) for item in value if str(item)}
    if isinstance(value, str):
        return {item for item in value.replace(",", " ").split() if item}
    return set()


def markdown_table(headers: list[str], rows: list[list[str]]) -> list[str]:
    if not rows:
        return ["_No matching rows._"]
    out = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    out.extend("| " + " | ".join(row) + " |" for row in rows)
    return out


def reader_key(row: dict[str, Any]) -> tuple[str, str, str, str, str]:
    return (
        str(row.get("surface", "")),
        str(row.get("language", "")),
        str(row.get("mode", "")),
        str(row.get("bounds", "")),
        str(row.get("mmap_strategy", "")),
    )


def reader_rows(run: BenchmarkRun) -> list[dict[str, Any]]:
    if run.kind != "reader-core":
        raise SystemExit(f"{run.label} is not a reader-core run")
    rows = [row for row in run.summary if isinstance(row, dict)]
    return sorted(rows, key=reader_sort_key)


def reader_rows_by_key(run: BenchmarkRun) -> dict[tuple[str, str, str, str, str], dict[str, Any]]:
    by_key: dict[tuple[str, str, str, str, str], dict[str, Any]] = {}
    for row in reader_rows(run):
        key = reader_key(row)
        if key in by_key:
            raise SystemExit(f"duplicate reader benchmark row key in {run.summary_path}: {key}")
        by_key[key] = row
    return by_key


def reader_sort_key(row: dict[str, Any]) -> tuple[int, int, int, int, str, str]:
    surface = str(row.get("surface", ""))
    language = str(row.get("language", ""))
    mode = str(row.get("mode", ""))
    bounds = str(row.get("bounds", ""))
    strategy = str(row.get("mmap_strategy", ""))
    return (
        SURFACE_ORDER.index(surface) if surface in SURFACE_ORDER else len(SURFACE_ORDER),
        LANGUAGE_ORDER.index(language) if language in LANGUAGE_ORDER else len(LANGUAGE_ORDER),
        READER_MODE_ORDER.index(mode) if mode in READER_MODE_ORDER else len(READER_MODE_ORDER),
        0 if bounds == "live" else 1 if bounds == "snapshot" else 2,
        strategy,
        mode,
    )


def reader_production_keys(surface: str) -> list[tuple[str, str, str, str, str]]:
    return [(surface, *item) for item in READER_PRODUCTION_ORDER]


def expected_reader_production_keys(run: BenchmarkRun, surface: str) -> list[tuple[str, str, str, str, str]]:
    languages = configured_languages(run)
    if not languages:
        languages = {str(row.get("language", "")) for row in reader_rows(run)}
    return [
        key
        for key in reader_production_keys(surface)
        if key[1] in languages and (surface == "file" or key[2] != "core-payloads")
    ]


def is_reader_production_key(key: tuple[str, str, str, str, str]) -> bool:
    surface, _, mode, _, _ = key
    return key in reader_production_keys(surface) and (surface == "file" or mode != "core-payloads")


def reader_production_items(
    run: BenchmarkRun,
    surface: str,
) -> list[tuple[tuple[str, str, str, str, str], dict[str, Any] | None]]:
    by_key = reader_rows_by_key(run)
    return [(key, by_key.get(key)) for key in expected_reader_production_keys(run, surface)]


def reader_reference(
    run: BenchmarkRun,
    items: list[tuple[tuple[str, str, str, str, str], dict[str, Any] | None]],
    language: str,
    mode: str,
) -> float | None:
    for key, row in items:
        if row is not None and key[1] == language and key[2] == mode:
            return required_float(row, "median_read_rows_per_second", run, f"{key[0]} {language} {mode}")
    return None


def render_reader_production(run: BenchmarkRun) -> list[str]:
    out: list[str] = ["## Production Comparison", ""]
    rendered_surfaces = 0
    surfaces = sorted({str(row.get("surface", "")) for row in reader_rows(run)}, key=surface_sort_key)
    for surface in surfaces:
        items = reader_production_items(run, surface)
        if not items:
            continue
        rendered_surfaces += 1
        systemd_ref = reader_reference(run, items, "systemd", "data")
        rust_ref = reader_reference(run, items, "rust", "sdk-payloads")
        table_rows = []
        for key, row in items:
            _, language, mode, bounds, strategy = key
            if row is None:
                access = access_from_parts(bounds, strategy)
                table_rows.append([surface, language, mode, access, "missing", "-", "-", "-", "-", "-"])
                continue
            context = f"{surface} {language} {mode}"
            median = required_float(row, "median_read_rows_per_second", run, context)
            table_rows.append(
                [
                    surface,
                    language,
                    mode,
                    access_label(row),
                    "measured",
                    fmt_int(median),
                    fmt_int(required_float(row, "min_read_rows_per_second", run, context)),
                    fmt_int(required_float(row, "max_read_rows_per_second", run, context)),
                    fmt_ratio_to(median, systemd_ref),
                    fmt_ratio_to(median, rust_ref),
                ]
            )
        out.extend([f"### {surface}", ""])
        out.extend(
            markdown_table(
                [
                    "surface",
                    "language",
                    "mode",
                    "access",
                    "status",
                    "median rows/s",
                    "min rows/s",
                    "max rows/s",
                    "vs systemd",
                    "vs rust sdk-payloads",
                ],
                table_rows,
            )
        )
        out.append("")
    if rendered_surfaces == 0:
        out.extend(["_No matching rows._", ""])
    return out


def render_reader_diagnostics(run: BenchmarkRun) -> list[str]:
    production = {
        key
        for surface in {str(row.get("surface", "")) for row in reader_rows(run)}
        for key in expected_reader_production_keys(run, surface)
    }
    rows = [row for row in reader_rows(run) if reader_key(row) not in production]
    table_rows = []
    for row in rows:
        surface = str(row.get("surface", ""))
        language = str(row.get("language", ""))
        mode = str(row.get("mode", ""))
        context = f"{surface} {language} {mode}"
        table_rows.append(
            [
                surface,
                language,
                mode,
                access_label(row),
                fmt_int(required_float(row, "median_read_rows_per_second", run, context)),
                fmt_int(required_float(row, "min_read_rows_per_second", run, context)),
                fmt_int(required_float(row, "max_read_rows_per_second", run, context)),
            ]
        )
    return [
        "## Diagnostic Modes",
        "",
        *markdown_table(
            ["surface", "language", "mode", "access", "median rows/s", "min rows/s", "max rows/s"],
            table_rows,
        ),
        "",
    ]


def surface_sort_key(surface: str) -> int:
    return SURFACE_ORDER.index(surface) if surface in SURFACE_ORDER else len(SURFACE_ORDER)


def render_reader_change(before: BenchmarkRun, after: BenchmarkRun) -> list[str]:
    before_rows = reader_rows_by_key(before)
    after_rows = reader_rows_by_key(after)
    keys = sorted(set(before_rows) & set(after_rows), key=change_key_sort)
    prod_rows = [
        render_reader_change_row(before, after, key, before_rows[key], after_rows[key])
        for key in keys
        if is_reader_production_key(key)
    ]
    diag_rows = [
        render_reader_change_row(before, after, key, before_rows[key], after_rows[key])
        for key in keys
        if not is_reader_production_key(key)
    ]
    out = ["## Change Comparison", "", "### Production Modes", ""]
    out.extend(reader_change_table(prod_rows))
    out.extend(["", "### Diagnostic Modes", ""])
    out.extend(reader_change_table(diag_rows))
    unmatched_rows = reader_unmatched_rows(before_rows, after_rows)
    if unmatched_rows:
        out.extend(["", "### Unmatched Rows", ""])
        out.extend(markdown_table(["side", "surface", "language", "mode", "access"], unmatched_rows))
    out.append("")
    return out


def change_key_sort(key: tuple[str, str, str, str, str]) -> tuple[int, int, int, int, str]:
    surface, language, mode, bounds, strategy = key
    production_order = reader_production_keys(surface)
    if is_reader_production_key(key):
        return (surface_sort_key(surface), 0, production_order.index(key), 0, "")
    return (
        surface_sort_key(surface),
        1,
        LANGUAGE_ORDER.index(language) if language in LANGUAGE_ORDER else len(LANGUAGE_ORDER),
        READER_MODE_ORDER.index(mode) if mode in READER_MODE_ORDER else len(READER_MODE_ORDER),
        f"{bounds}/{strategy}",
    )


def render_reader_change_row(
    before_run: BenchmarkRun,
    after_run: BenchmarkRun,
    key: tuple[str, str, str, str, str],
    before: dict[str, Any],
    after: dict[str, Any],
) -> list[str]:
    surface, language, mode, bounds, strategy = key
    context = f"{surface} {language} {mode}"
    before_rate = required_float(before, "median_read_rows_per_second", before_run, context)
    after_rate = required_float(after, "median_read_rows_per_second", after_run, context)
    delta, ratio = fmt_delta(before_rate, after_rate)
    access = access_from_parts(bounds, strategy)
    return [
        surface,
        language,
        mode,
        access,
        fmt_int(before_rate),
        fmt_int(after_rate),
        delta,
        ratio,
    ]


def reader_change_table(rows: list[list[str]]) -> list[str]:
    return markdown_table(
        ["surface", "language", "mode", "access", "before rows/s", "after rows/s", "delta", "ratio"],
        rows,
    )


def reader_unmatched_rows(
    before_rows: dict[tuple[str, str, str, str, str], dict[str, Any]],
    after_rows: dict[tuple[str, str, str, str, str], dict[str, Any]],
) -> list[list[str]]:
    rows: list[list[str]] = []
    for side, keys in (
        ("before only", sorted(set(before_rows) - set(after_rows), key=change_key_sort)),
        ("after only", sorted(set(after_rows) - set(before_rows), key=change_key_sort)),
    ):
        for surface, language, mode, bounds, strategy in keys:
            access = access_from_parts(bounds, strategy)
            rows.append([side, surface, language, mode, access])
    return rows


def writer_change_table(rows: list[list[str]]) -> list[str]:
    return markdown_table(
        ["surface", "language", "api", "access", "before rows/s", "after rows/s", "delta", "ratio"],
        rows,
    )


def sorted_languages(languages: set[str]) -> list[str]:
    known = [language for language in LANGUAGE_ORDER if language in languages]
    unknown = sorted(languages - set(LANGUAGE_ORDER))
    return [*known, *unknown]


def writer_api(data: dict[str, Any]) -> str:
    return join_values(data.get("api_modes"))


def writer_access(data: dict[str, Any]) -> str:
    return join_values(data.get("mmap_strategies"))


def writer_rows(run: BenchmarkRun) -> list[tuple[str, dict[str, Any]]]:
    if run.kind != "writer-core":
        raise SystemExit(f"{run.label} is not a writer-core run")
    if not isinstance(run.summary, dict):
        raise SystemExit(f"writer-core summary is not an object: {run.summary_path}")
    rows = [(language, data) for language, data in run.summary.items() if isinstance(data, dict)]
    return sorted(
        rows,
        key=lambda item: LANGUAGE_ORDER.index(item[0]) if item[0] in LANGUAGE_ORDER else len(LANGUAGE_ORDER),
    )


def expected_writer_languages(run: BenchmarkRun) -> list[str]:
    languages = configured_languages(run)
    if not languages:
        languages = {language for language, _ in writer_rows(run)}
    return sorted_languages(languages)


def render_writer_production(run: BenchmarkRun) -> list[str]:
    rows = dict(writer_rows(run))
    rust_ref = (
        required_float(rows["rust"], "append_rows_per_second_median", run, "rust")
        if "rust" in rows
        else None
    )
    table_rows = []
    for language in expected_writer_languages(run):
        data = rows.get(language)
        if data is None:
            table_rows.append(["writer-core", language, "-", "-", "missing", "-", "-", "-", "-", "-"])
            continue
        median = required_float(data, "append_rows_per_second_median", run, language)
        table_rows.append(
            [
                "writer-core",
                language,
                writer_api(data),
                writer_access(data),
                "measured",
                fmt_int(median),
                fmt_int(required_float(data, "append_rows_per_second_min", run, language)),
                fmt_int(required_float(data, "append_rows_per_second_max", run, language)),
                fmt_ratio(optional_float(data, "systemd_append_ratio_median", run, language)),
                fmt_ratio_to(median, rust_ref),
            ]
        )
    return [
        "## Production Comparison",
        "",
        *markdown_table(
            [
                "surface",
                "language",
                "api",
                "access",
                "status",
                "median rows/s",
                "min rows/s",
                "max rows/s",
                "vs systemd",
                "vs rust",
            ],
            table_rows,
        ),
        "",
    ]


def render_writer_change(before: BenchmarkRun, after: BenchmarkRun) -> list[str]:
    before_rows = dict(writer_rows(before))
    after_rows = dict(writer_rows(after))
    rows = []
    common = set(before_rows) & set(after_rows)
    for language in sorted_languages(common):
        before_rate = required_float(
            before_rows[language],
            "append_rows_per_second_median",
            before,
            language,
        )
        after_rate = required_float(
            after_rows[language],
            "append_rows_per_second_median",
            after,
            language,
        )
        delta, ratio = fmt_delta(before_rate, after_rate)
        rows.append(
            [
                "writer-core",
                language,
                writer_api(after_rows[language]),
                writer_access(after_rows[language]),
                fmt_int(before_rate),
                fmt_int(after_rate),
                delta,
                ratio,
            ]
        )
    out = ["## Change Comparison", "", *writer_change_table(rows)]
    unmatched_rows = writer_unmatched_rows(before_rows, after_rows)
    if unmatched_rows:
        out.extend(["", "### Unmatched Rows", ""])
        out.extend(markdown_table(["side", "language", "api", "access"], unmatched_rows))
    config_rows = writer_config_difference_rows(before_rows, after_rows)
    if config_rows:
        out.extend(["", "### Configuration Differences", ""])
        out.extend(
            markdown_table(
                ["language", "before api", "before access", "after api", "after access"],
                config_rows,
            )
        )
    out.append("")
    return out


def writer_unmatched_rows(
    before_rows: dict[str, dict[str, Any]],
    after_rows: dict[str, dict[str, Any]],
) -> list[list[str]]:
    rows: list[list[str]] = []
    for side, keys, source in (
        ("before only", sorted_languages(set(before_rows) - set(after_rows)), before_rows),
        ("after only", sorted_languages(set(after_rows) - set(before_rows)), after_rows),
    ):
        for language in keys:
            rows.append([side, language, writer_api(source[language]), writer_access(source[language])])
    return rows


def writer_config_difference_rows(
    before_rows: dict[str, dict[str, Any]],
    after_rows: dict[str, dict[str, Any]],
) -> list[list[str]]:
    rows: list[list[str]] = []
    for language in sorted_languages(set(before_rows) & set(after_rows)):
        before_api = writer_api(before_rows[language])
        before_access = writer_access(before_rows[language])
        after_api = writer_api(after_rows[language])
        after_access = writer_access(after_rows[language])
        if before_api != after_api or before_access != after_access:
            rows.append([language, before_api, before_access, after_api, after_access])
    return rows


def render_identity(run: BenchmarkRun | None, before: BenchmarkRun | None, after: BenchmarkRun | None) -> list[str]:
    rows = []
    for role, item in (("run", run), ("before", before), ("after", after)):
        if item is None:
            continue
        parameters = item.manifest.get("parameters")
        if not isinstance(parameters, dict):
            parameters = {}
        environment = item.manifest.get("environment")
        if not isinstance(environment, dict):
            environment = {}
        rows.extend(
            [
                [f"{role} label", item.label],
                [f"{role} kind", item.kind],
                [f"{role} run dir", str(item.run_dir)],
                [f"{role} summary", str(item.summary_path)],
            ]
        )
        if item.manifest_path:
            rows.append([f"{role} manifest", str(item.manifest_path)])
        created_at = (
            item.manifest.get("created_at")
            or parameters.get("created_at")
            or environment.get("timestamp_utc")
        )
        if created_at:
            rows.append([f"{role} created at", str(created_at)])
        host = item.manifest.get("host") or environment.get("host")
        if host:
            rows.append([f"{role} host", str(host)])
    return ["## Run Identity", "", *markdown_table(["field", "value"], rows), ""]


def config_pairs(run: BenchmarkRun) -> list[list[str]]:
    manifest = run.manifest
    if run.kind == "writer-core":
        source = manifest.get("parameters")
        if not isinstance(source, dict):
            source = {}
        keys = [
            "format",
            "compression",
            "fss",
            "final_state",
            "rows",
            "fields_per_row",
            "repetitions",
            "warmups",
            "languages",
            "max_size_bytes",
            "api_mode",
            "live_publish_every_entries",
            "hash_table_sizing",
            "append_timer_excludes",
        ]
    else:
        source = manifest
        keys = [
            "format",
            "final_state",
            "rows",
            "directory_rows",
            "max_size_bytes",
            "directory_max_size_bytes",
            "window_size",
            "direction",
            "languages",
            "timer_excludes",
        ]
    rows = []
    for key in keys:
        if key in source:
            value = source[key]
            if isinstance(value, list):
                value = ", ".join(map(str, value))
            elif isinstance(value, dict):
                value = json.dumps(value, sort_keys=True)
            rows.append([key, str(value)])
    return rows


def render_configuration(run: BenchmarkRun | None, after: BenchmarkRun | None) -> list[str]:
    source = after or run
    if source is None:
        return []
    return ["## Configuration", "", *markdown_table(["setting", "value"], config_pairs(source)), ""]


def render_raw_evidence(run: BenchmarkRun | None, before: BenchmarkRun | None, after: BenchmarkRun | None) -> list[str]:
    rows = []
    for role, item in (("run", run), ("before", before), ("after", after)):
        if item is None:
            continue
        rows.append([role, "summary", str(item.summary_path)])
        if item.manifest_path:
            rows.append([role, "manifest", str(item.manifest_path)])
        runs_path = item.run_dir / "runs.jsonl"
        if runs_path.exists():
            rows.append([role, "runs", str(runs_path)])
    return ["## Raw Evidence", "", *markdown_table(["role", "artifact", "path"], rows), ""]


def render_report(
    *,
    title: str,
    run: BenchmarkRun | None,
    before: BenchmarkRun | None,
    after: BenchmarkRun | None,
    conclusion: str,
    conclusion_note: str,
) -> str:
    primary = validate_report_inputs(run, before, after, conclusion)

    out = [f"# {title}", ""]
    out.extend(render_identity(run, before, after))
    out.extend(render_configuration(run, after))
    out.extend(render_primary_results(primary, before, after))
    out.extend(render_conclusion(conclusion, conclusion_note))
    out.extend(render_raw_evidence(run, before, after))
    return "\n".join(out).rstrip() + "\n"


def validate_report_inputs(
    run: BenchmarkRun | None,
    before: BenchmarkRun | None,
    after: BenchmarkRun | None,
    conclusion: str,
) -> BenchmarkRun:
    if conclusion not in CONCLUSION_CHOICES:
        raise SystemExit(f"unsupported conclusion label: {conclusion}")
    primary = after or run
    if primary is None:
        raise SystemExit("provide --run or both --before and --after")
    if before is not None and after is None:
        raise SystemExit("--before requires --after")
    if after is not None and before is None:
        raise SystemExit("--after requires --before")
    if before is not None and before.kind != after.kind:
        raise SystemExit(f"before/after benchmark kinds differ: {before.kind} vs {after.kind}")
    return primary


def render_primary_results(
    primary: BenchmarkRun,
    before: BenchmarkRun | None,
    after: BenchmarkRun | None,
) -> list[str]:
    out: list[str] = []
    if primary.kind == "reader-core":
        out.extend(render_reader_production(primary))
        out.extend(render_reader_diagnostics(primary))
        if before and after:
            out.extend(render_reader_change(before, after))
    elif primary.kind == "writer-core":
        out.extend(render_writer_production(primary))
        if before and after:
            out.extend(render_writer_change(before, after))
    else:
        raise SystemExit(f"unsupported benchmark kind: {primary.kind}")
    return out


def render_conclusion(conclusion: str, conclusion_note: str) -> list[str]:
    out = [
        "## Conclusion",
        "",
        f"- Verdict: {CONCLUSION_CHOICES[conclusion]}",
    ]
    if conclusion_note:
        out.append(f"- Note: {conclusion_note}")
    out.append("")
    return out


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=Path, help="Benchmark run directory, summary.json, or report.json")
    parser.add_argument("--before", type=Path, help="Before benchmark run for change comparison")
    parser.add_argument("--after", type=Path, help="After benchmark run for change comparison")
    parser.add_argument("--title", default="Benchmark Report")
    parser.add_argument("--out", type=Path, help="Write Markdown report to this path instead of stdout")
    parser.add_argument(
        "--conclusion",
        choices=sorted(CONCLUSION_CHOICES),
        default="not-assessed",
        help="Explicit benchmark conclusion label; the tool does not infer this.",
    )
    parser.add_argument("--conclusion-note", default="", help="Optional human-written conclusion note")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.run and (args.before or args.after):
        raise SystemExit("--run cannot be combined with --before/--after")
    run = load_run(args.run, "run") if args.run else None
    before = load_run(args.before, "before") if args.before else None
    after = load_run(args.after, "after") if args.after else None
    report = render_report(
        title=args.title,
        run=run,
        before=before,
        after=after,
        conclusion=args.conclusion,
        conclusion_note=args.conclusion_note,
    )
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(report, encoding="utf-8")
    else:
        sys.stdout.write(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
