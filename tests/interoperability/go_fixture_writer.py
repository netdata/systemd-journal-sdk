#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write synthetic journal fixtures through the in-repo Go SDK."""

from __future__ import annotations

import base64
import json
import os
import subprocess  # nosec B404
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BUILDER_DIR = LOCAL_DIR / "go-fixture-writer"
LIVE_BUILDER_DIR = LOCAL_DIR / "go-live-fixture-writer"
SPEC_DIR = LOCAL_DIR / "go-fixture-specs"
BIN_DIR = LOCAL_DIR / "bin"
GO_MODULE_DIR = REPO_ROOT / "go"


def _read_go_directive(go_mod_path: Path = GO_MODULE_DIR / "go.mod") -> str:
    for raw in go_mod_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("go "):
            return line.split(None, 1)[1].strip()
    raise RuntimeError(f"cannot find go directive in {go_mod_path}")


def _read_go_requirements(go_mod_path: Path = GO_MODULE_DIR / "go.mod") -> list[str]:
    requirements: list[str] = []
    in_block = False
    for raw in go_mod_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line == "require (":
            in_block = True
            continue
        if in_block:
            if line == ")":
                break
            if line:
                requirements.append(line)
            continue
        if line.startswith("require "):
            requirements.append(line.split(None, 1)[1].strip())
    return requirements


def _render_go_mod() -> str:
    requirements = "\n".join(
        ["\tgithub.com/netdata/systemd-journal-sdk/go v0.0.0"]
        + [f"\t{requirement}" for requirement in _read_go_requirements()]
    )
    return (
        "module fixturewriter\n\n"
        f"go {_read_go_directive()}\n\n"
        "require (\n"
        f"{requirements}\n"
        ")\n\n"
        f"replace github.com/netdata/systemd-journal-sdk/go => {GO_MODULE_DIR}\n"
    )


def _render_main_go() -> str:
    return r'''
package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	journal "github.com/netdata/systemd-journal-sdk/go/journal"
)

type fieldSpec struct {
	Name     string `json:"name"`
	ValueB64 string `json:"value_b64"`
}

type entrySpec struct {
	RealtimeUsec  uint64      `json:"realtime_usec"`
	MonotonicUsec uint64      `json:"monotonic_usec"`
	BootID        string      `json:"boot_id"`
	Fields        []fieldSpec `json:"fields"`
}

type sealSpec struct {
	SeedB64      string `json:"seed_b64"`
	IntervalUsec uint64 `json:"interval_usec"`
	StartUsec    uint64 `json:"start_usec"`
}

type spec struct {
	Path                      string      `json:"path"`
	MachineID                 string      `json:"machine_id"`
	BootID                    string      `json:"boot_id"`
	SeqnumID                  string      `json:"seqnum_id"`
	FileID                    string      `json:"file_id"`
	Compact                   bool        `json:"compact"`
	Compression               string      `json:"compression"`
	CompressionThresholdBytes int         `json:"compression_threshold_bytes"`
	Seal                      *sealSpec   `json:"seal"`
	Entries                   []entrySpec `json:"entries"`
}

func parseUUID(value string) (journal.UUID, error) {
	if value == "" {
		return journal.UUID{}, nil
	}
	return journal.ParseUUID(value)
}

func compressionID(name string) (int, error) {
	switch name {
	case "", "none":
		return journal.CompressionNone, nil
	case "zstd":
		return journal.CompressionZSTD, nil
	case "xz":
		return journal.CompressionXZ, nil
	case "lz4":
		return journal.CompressionLZ4, nil
	default:
		return 0, fmt.Errorf("unsupported compression %q", name)
	}
}

func readSpec(path string) (spec, error) {
	var s spec
	data, err := os.ReadFile(path)
	if err != nil {
		return s, err
	}
	if err := json.Unmarshal(data, &s); err != nil {
		return s, err
	}
	return s, nil
}

func buildOptions(s spec) (journal.Options, error) {
	machineID, err := parseUUID(s.MachineID)
	if err != nil {
		return journal.Options{}, err
	}
	bootID, err := parseUUID(s.BootID)
	if err != nil {
		return journal.Options{}, err
	}
	seqnumID, err := parseUUID(s.SeqnumID)
	if err != nil {
		return journal.Options{}, err
	}
	fileID, err := parseUUID(s.FileID)
	if err != nil {
		return journal.Options{}, err
	}
	compression, err := compressionID(s.Compression)
	if err != nil {
		return journal.Options{}, err
	}
	opts := journal.Options{
		MachineID:              machineID,
		BootID:                 bootID,
		SeqnumID:               seqnumID,
		FileID:                 fileID,
		Compact:                s.Compact,
		Compression:            compression,
		CompressThresholdBytes: s.CompressionThresholdBytes,
	}
	if s.Seal != nil {
		seed, err := base64.StdEncoding.DecodeString(s.Seal.SeedB64)
		if err != nil {
			return journal.Options{}, err
		}
		opts.Seal = &journal.SealOptions{
			Seed:         seed,
			IntervalUsec: s.Seal.IntervalUsec,
			StartUsec:    s.Seal.StartUsec,
		}
	}
	return opts, nil
}

func appendEntry(w *journal.Writer, entry entrySpec) error {
	fields := make([]journal.Field, 0, len(entry.Fields))
	for _, item := range entry.Fields {
		value, err := base64.StdEncoding.DecodeString(item.ValueB64)
		if err != nil {
			return err
		}
		fields = append(fields, journal.Field{Name: item.Name, Value: value})
	}
	opts := journal.EntryOptions{
		RealtimeUsec:     entry.RealtimeUsec,
		RealtimeUsecSet:  true,
		MonotonicUsec:    entry.MonotonicUsec,
		MonotonicUsecSet: true,
	}
	if entry.BootID != "" {
		bootID, err := journal.ParseUUID(entry.BootID)
		if err != nil {
			return err
		}
		opts.BootID = bootID
	}
	return w.Append(fields, opts)
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: fixturewriter <spec.json>")
		os.Exit(2)
	}
	s, err := readSpec(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	opts, err := buildOptions(s)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := os.MkdirAll(filepath.Dir(s.Path), 0o755); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	w, err := journal.Create(s.Path, opts)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	closed := false
	defer func() {
		if !closed {
			_ = w.Close()
		}
	}()
	for _, entry := range s.Entries {
		if err := appendEntry(w, entry); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
	}
	if err := w.CloseOffline(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	closed = true
}
'''


def _render_live_main_go() -> str:
    return r'''
package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	journal "github.com/netdata/systemd-journal-sdk/go/journal"
)

type fieldSpec struct {
	Name     string `json:"name"`
	ValueB64 string `json:"value_b64"`
}

type entrySpec struct {
	RealtimeUsec  uint64      `json:"realtime_usec"`
	MonotonicUsec uint64      `json:"monotonic_usec"`
	BootID        string      `json:"boot_id"`
	Fields        []fieldSpec `json:"fields"`
}

type spec struct {
	Path                   string      `json:"path"`
	ReadyFile              string      `json:"ready_file"`
	MachineID              string      `json:"machine_id"`
	BootID                 string      `json:"boot_id"`
	SeqnumID               string      `json:"seqnum_id"`
	Initial                []entrySpec `json:"initial"`
	Appends                []entrySpec `json:"appends"`
	AppendDelayMillis      int         `json:"append_delay_millis"`
	InterAppendDelayMillis int         `json:"inter_append_delay_millis"`
}

func parseUUID(value string) (journal.UUID, error) {
	if value == "" {
		return journal.UUID{}, nil
	}
	return journal.ParseUUID(value)
}

func readSpec(path string) (spec, error) {
	var s spec
	data, err := os.ReadFile(path)
	if err != nil {
		return s, err
	}
	if err := json.Unmarshal(data, &s); err != nil {
		return s, err
	}
	return s, nil
}

func buildOptions(s spec) (journal.Options, error) {
	machineID, err := parseUUID(s.MachineID)
	if err != nil {
		return journal.Options{}, err
	}
	bootID, err := parseUUID(s.BootID)
	if err != nil {
		return journal.Options{}, err
	}
	seqnumID, err := parseUUID(s.SeqnumID)
	if err != nil {
		return journal.Options{}, err
	}
	return journal.Options{
		MachineID: machineID,
		BootID:    bootID,
		SeqnumID:  seqnumID,
	}, nil
}

func appendEntry(w *journal.Writer, entry entrySpec) error {
	fields := make([]journal.Field, 0, len(entry.Fields))
	for _, item := range entry.Fields {
		value, err := base64.StdEncoding.DecodeString(item.ValueB64)
		if err != nil {
			return err
		}
		fields = append(fields, journal.Field{Name: item.Name, Value: value})
	}
	opts := journal.EntryOptions{
		RealtimeUsec:     entry.RealtimeUsec,
		RealtimeUsecSet:  true,
		MonotonicUsec:    entry.MonotonicUsec,
		MonotonicUsecSet: true,
	}
	if entry.BootID != "" {
		bootID, err := journal.ParseUUID(entry.BootID)
		if err != nil {
			return err
		}
		opts.BootID = bootID
	}
	return w.Append(fields, opts)
}

func appendAndSync(w *journal.Writer, entries []entrySpec) error {
	for _, entry := range entries {
		if err := appendEntry(w, entry); err != nil {
			return err
		}
		if err := w.Sync(); err != nil {
			return err
		}
	}
	return nil
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: livefixturewriter <spec.json>")
		os.Exit(2)
	}
	s, err := readSpec(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	opts, err := buildOptions(s)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := os.MkdirAll(filepath.Dir(s.Path), 0o755); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := os.MkdirAll(filepath.Dir(s.ReadyFile), 0o755); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	w, err := journal.Create(s.Path, opts)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	closed := false
	defer func() {
		if !closed {
			_ = w.Close()
		}
	}()
	if err := appendAndSync(w, s.Initial); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := w.Sync(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := os.WriteFile(s.ReadyFile, []byte("ready\n"), 0o600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if s.AppendDelayMillis > 0 {
		time.Sleep(time.Duration(s.AppendDelayMillis) * time.Millisecond)
	}
	for _, entry := range s.Appends {
		if err := appendEntry(w, entry); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		if err := w.Sync(); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		if s.InterAppendDelayMillis > 0 {
			time.Sleep(time.Duration(s.InterAppendDelayMillis) * time.Millisecond)
		}
	}
	if err := w.Close(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	closed = true
}
'''


def _ensure_builder() -> None:
    BUILDER_DIR.mkdir(parents=True, exist_ok=True)
    (BUILDER_DIR / "go.mod").write_text(_render_go_mod(), encoding="utf-8")
    (BUILDER_DIR / "go.sum").write_text((GO_MODULE_DIR / "go.sum").read_text(encoding="utf-8"), encoding="utf-8")
    (BUILDER_DIR / "main.go").write_text(_render_main_go(), encoding="utf-8")


def _ensure_live_builder(env: dict[str, str] | None = None) -> Path:
    LIVE_BUILDER_DIR.mkdir(parents=True, exist_ok=True)
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    (LIVE_BUILDER_DIR / "go.mod").write_text(_render_go_mod(), encoding="utf-8")
    (LIVE_BUILDER_DIR / "go.sum").write_text((GO_MODULE_DIR / "go.sum").read_text(encoding="utf-8"), encoding="utf-8")
    (LIVE_BUILDER_DIR / "main.go").write_text(_render_live_main_go(), encoding="utf-8")
    binary = BIN_DIR / "go-live-fixture-writer"
    result = subprocess.run(  # nosec B603
        ["go", "build", "-o", str(binary), "."],  # nosemgrep
        cwd=str(LIVE_BUILDER_DIR),
        env=_env(env),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"build go live fixture writer failed with exit {result.returncode}\n"
            f"stdout:\n{result.stdout[-2000:]}\n"
            f"stderr:\n{result.stderr[-2000:]}"
        )
    return binary


def _env(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = os.environ.copy()
    local = REPO_ROOT / ".local"
    env.setdefault("GOMODCACHE", str(local / "go" / "pkg" / "mod"))
    env.setdefault("GOCACHE", str(local / "go-build"))
    env.setdefault("GOPATH", str(local / "go"))
    if extra:
        env.update(extra)
    return env


def _field(name: str, value: str | bytes) -> dict[str, str]:
    raw = value.encode("utf-8") if isinstance(value, str) else bytes(value)
    return {
        "name": name,
        "value_b64": base64.b64encode(raw).decode("ascii"),
    }


def _entry_spec(entry: dict[str, Any]) -> dict[str, Any]:
    return {
        "realtime_usec": int(entry["realtime_usec"]),
        "monotonic_usec": int(entry["monotonic_usec"]),
        "boot_id": entry.get("boot_id", ""),
        "fields": [_field(name, value) for name, value in entry["fields"]],
    }


def write_journal_file(
    path: Path,
    *,
    machine_id: str,
    boot_id: str,
    seqnum_id: str,
    entries: list[dict[str, Any]],
    file_id: str = "",
    compact: bool = False,
    compression: str = "none",
    compression_threshold_bytes: int = 512,
    seal: dict[str, Any] | None = None,
    env: dict[str, str] | None = None,
    timeout: int = 120,
) -> None:
    """Write one journal fixture file using the in-repo Go writer."""
    _ensure_builder()
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    serialized_entries = []
    for entry in entries:
        serialized_entries.append({
            "realtime_usec": int(entry["realtime_usec"]),
            "monotonic_usec": int(entry["monotonic_usec"]),
            "boot_id": entry.get("boot_id", ""),
            "fields": [_field(name, value) for name, value in entry["fields"]],
        })
    serialized_seal = None
    if seal is not None:
        seed = seal["seed"]
        seed_bytes = seed.encode("latin-1") if isinstance(seed, str) else bytes(seed)
        serialized_seal = {
            "seed_b64": base64.b64encode(seed_bytes).decode("ascii"),
            "interval_usec": int(seal["interval_usec"]),
            "start_usec": int(seal["start_usec"]),
        }
    spec = {
        "path": str(path),
        "machine_id": machine_id,
        "boot_id": boot_id,
        "seqnum_id": seqnum_id,
        "file_id": file_id,
        "compact": compact,
        "compression": compression,
        "compression_threshold_bytes": compression_threshold_bytes,
        "seal": serialized_seal,
        "entries": serialized_entries,
    }
    spec_path = SPEC_DIR / (path.name.replace("/", "_") + ".json")
    spec_path.write_text(json.dumps(spec, indent=2, sort_keys=True), encoding="utf-8")
    result = subprocess.run(  # nosec B603
        ["go", "run", ".", str(spec_path)],
        cwd=str(BUILDER_DIR),
        env=_env(env),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"go fixture writer failed for {path} with exit {result.returncode}\n"
            f"stdout:\n{result.stdout[-2000:]}\n"
            f"stderr:\n{result.stderr[-2000:]}"
        )


def start_live_journal_writer(
    path: Path,
    *,
    ready_file: Path,
    machine_id: str,
    boot_id: str,
    seqnum_id: str,
    initial_entries: list[dict[str, Any]],
    append_entries: list[dict[str, Any]],
    append_delay_millis: int = 250,
    inter_append_delay_millis: int = 150,
    env: dict[str, str] | None = None,
) -> subprocess.Popen[str]:
    """Start a Go writer that leaves the file active while follow readers run."""
    binary = _ensure_live_builder(env)
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    spec_path = SPEC_DIR / ("live-" + path.name.replace("/", "_") + ".json")
    spec = {
        "path": str(path),
        "ready_file": str(ready_file),
        "machine_id": machine_id,
        "boot_id": boot_id,
        "seqnum_id": seqnum_id,
        "initial": [_entry_spec(entry) for entry in initial_entries],
        "appends": [_entry_spec(entry) for entry in append_entries],
        "append_delay_millis": append_delay_millis,
        "inter_append_delay_millis": inter_append_delay_millis,
    }
    spec_path.write_text(json.dumps(spec, indent=2, sort_keys=True), encoding="utf-8")
    return subprocess.Popen(  # nosec B603
        [str(binary), str(spec_path)],  # nosemgrep
        cwd=str(REPO_ROOT),
        env=_env(env),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
