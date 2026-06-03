#!/usr/bin/env python3
"""Build and run deterministic dataset ingesters."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DATASETS = ROOT / "tests" / "datasets"
CORRECTNESS = DATASETS / "correctness" / "corpus.jsonl"
REJECTIONS = DATASETS / "rejections" / "corpus.jsonl"
OUT = ROOT / ".local" / "datasets" / "ingesters"
BIN = OUT / "bin"

LANGUAGES = ("systemd", "rust", "go", "node", "python")
SEQNUM_ID = "22222222222222222222222222222222"
DEFAULT_MAX_SIZE_BYTES = 64 * 1024 * 1024


def run(cmd: list[str], *, cwd: Path = ROOT, env: dict[str, str] | None = None) -> dict:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.run(cmd, cwd=cwd, env=merged_env, text=True, capture_output=True)  # nosec B603  # nosemgrep
    return {
        "cmd": cmd,
        "cwd": str(cwd),
        "returncode": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
    }


def ensure_bins(language: str) -> tuple[Path | str, dict]:
    BIN.mkdir(parents=True, exist_ok=True)
    if language == "python":
        return sys.executable, {"script": str(ROOT / "python" / "cmd" / "dataset_ingester.py")}
    if language == "node":
        return "node", {"script": str(ROOT / "node" / "cmd" / "dataset_ingester.js")}
    if language == "go":
        output = BIN / "go-dataset-ingester"
        result = run(
            ["go", "build", "-o", str(output), "./internal/testcmd/dataset_ingester"],
            cwd=ROOT / "go",
            env={
                "GOMODCACHE": str(ROOT / ".local" / "gomodcache"),
                "GOPATH": str(ROOT / ".local" / "gopath"),
            },
        )
        if result["returncode"] != 0:
            raise RuntimeError(json.dumps({"go_build": result}, indent=2))
        return output, {}
    if language == "rust":
        result = run(
            ["cargo", "build", "-p", "dataset_ingester"],
            cwd=ROOT / "rust",
            env={
                "CARGO_TARGET_DIR": str(ROOT / ".local" / "cargo-target"),
                "CARGO_HOME": str(ROOT / ".local" / "cargo-home"),
            },
        )
        if result["returncode"] != 0:
            raise RuntimeError(json.dumps({"rust_build": result}, indent=2))
        return ROOT / ".local" / "cargo-target" / "debug" / "dataset_ingester", {}
    if language == "systemd":
        result = run([str(ROOT / "tests" / "datasets" / "ingesters" / "systemd" / "build.sh")])
        if result["returncode"] != 0:
            raise RuntimeError(json.dumps({"systemd_build": result}, indent=2))
        return Path(result["stdout"].strip().splitlines()[-1]), {}
    raise ValueError(language)


def first_accepted_realtime(dataset: Path) -> int:
    with dataset.open("r", encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("record_type") == "accepted":
                return int(record["realtime_usec"])
    return 0


def final_journal_path(output: Path, final_state: str, dataset: Path) -> Path:
    if final_state != "archived":
        return output
    prefix = output.name[:-len(".journal")] if output.name.endswith(".journal") else output.name
    return output.with_name(
        f"{prefix}@{SEQNUM_ID}-0000000000000001-{first_accepted_realtime(dataset):016x}.journal"
    )


def ingester_command(
    language: str,
    binary: Path | str,
    metadata: dict,
    dataset: Path,
    output: Path,
    rejection: bool,
    final_state: str,
    max_size_bytes: int | None,
) -> list[str]:
    if language in {"python", "node"}:
        cmd = [str(binary), metadata["script"]]
    else:
        cmd = [str(binary)]
    cmd += ["--dataset", str(dataset), "--output", str(output)]
    if rejection:
        cmd.append("--rejection-mode")
    cmd += ["--final-state", final_state]
    if max_size_bytes is not None:
        cmd += ["--max-size-bytes", str(max_size_bytes)]
    return cmd


def verify_journal(path: Path) -> dict:
    if shutil.which("journalctl") is None:
        return {"returncode": 127, "stdout": "", "stderr": "journalctl not found"}
    return run(["journalctl", "--verify", "--file", str(path)])


def run_language(language: str, both: bool, final_state: str, max_size_bytes: int | None) -> dict:
    binary, metadata = ensure_bins(language)
    lang_out = OUT / language if final_state == "online" else OUT / final_state / language
    lang_out.mkdir(parents=True, exist_ok=True)
    result: dict[str, dict] = {}

    accepted_output = lang_out / "correctness.journal"
    actual_output = final_journal_path(accepted_output, final_state, CORRECTNESS)
    for path in {accepted_output, actual_output}:
        path.unlink(missing_ok=True)
    accepted = run(
        ingester_command(
            language,
            binary,
            metadata,
            CORRECTNESS,
            accepted_output,
            False,
            final_state,
            max_size_bytes,
        )
    )
    result["accepted"] = accepted
    if accepted["returncode"] == 0:
        result["verify"] = verify_journal(actual_output)
        result["journal"] = {"path": str(actual_output)}

    if both:
        rejection_output = lang_out / "rejections.journal"
        rejection_output.unlink(missing_ok=True)
        result["rejections"] = run(
            ingester_command(
                language,
                binary,
                metadata,
                REJECTIONS,
                rejection_output,
                True,
                "online",
                max_size_bytes,
            )
        )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--language", choices=LANGUAGES)
    parser.add_argument("--both", action="store_true", help="run accepted and rejection corpora")
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument(
        "--max-size-bytes",
        type=int,
        default=DEFAULT_MAX_SIZE_BYTES,
        help="systemd max-size value used for hash table sizing across all ingesters",
    )
    args = parser.parse_args()

    languages = [args.language] if args.language else list(LANGUAGES)
    summary = {}
    failed = False

    dataset_check = run([sys.executable, str(DATASETS / "validate.py")])
    summary["dataset_validate"] = dataset_check
    if dataset_check["returncode"] != 0:
        failed = True

    for language in languages:
        try:
            language_result = run_language(language, args.both, args.final_state, args.max_size_bytes)
        except Exception as err:
            language_result = {"exception": str(err)}
            failed = True
        else:
            for value in language_result.values():
                if isinstance(value, dict) and value.get("returncode", 0) != 0:
                    failed = True
        summary[language] = language_result

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
