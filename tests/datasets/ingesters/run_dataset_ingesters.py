#!/usr/bin/env python3
"""Build and run deterministic dataset ingesters."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DATASETS = ROOT / "tests" / "datasets"
CORRECTNESS = DATASETS / "correctness" / "corpus.jsonl"
REJECTIONS = DATASETS / "rejections" / "corpus.jsonl"
OUT = ROOT / ".local" / "datasets" / "ingesters"
BIN = OUT / "bin"

LANGUAGES = ("systemd", "rust", "go", "node", "python")


def run(cmd: list[str], *, cwd: Path = ROOT, env: dict[str, str] | None = None) -> dict:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    proc = subprocess.run(cmd, cwd=cwd, env=merged_env, text=True, capture_output=True)
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


def ingester_command(language: str, binary: Path | str, metadata: dict, dataset: Path, output: Path, rejection: bool) -> list[str]:
    if language in {"python", "node"}:
        cmd = [str(binary), metadata["script"]]
    else:
        cmd = [str(binary)]
    cmd += ["--dataset", str(dataset), "--output", str(output)]
    if rejection:
        cmd.append("--rejection-mode")
    return cmd


def verify_journal(path: Path) -> dict:
    if shutil.which("journalctl") is None:
        return {"returncode": 127, "stdout": "", "stderr": "journalctl not found"}
    return run(["journalctl", "--verify", "--file", str(path)])


def run_language(language: str, both: bool) -> dict:
    binary, metadata = ensure_bins(language)
    lang_out = OUT / language
    lang_out.mkdir(parents=True, exist_ok=True)
    result: dict[str, dict] = {}

    accepted_output = lang_out / "correctness.journal"
    accepted = run(ingester_command(language, binary, metadata, CORRECTNESS, accepted_output, False))
    result["accepted"] = accepted
    if accepted["returncode"] == 0:
        result["verify"] = verify_journal(accepted_output)

    if both:
        rejection_output = lang_out / "rejections.journal"
        result["rejections"] = run(ingester_command(language, binary, metadata, REJECTIONS, rejection_output, True))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--language", choices=LANGUAGES)
    parser.add_argument("--both", action="store_true", help="run accepted and rejection corpora")
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
            language_result = run_language(language, args.both)
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
