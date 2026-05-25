package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestProbeReportsImplementedZstdSupport(t *testing.T) {
	var buf bytes.Buffer
	probeAdapter(&buf)

	var info struct {
		Capabilities map[string]bool `json:"capabilities"`
	}
	if err := json.Unmarshal(buf.Bytes(), &info); err != nil {
		t.Fatalf("decode probe output: %v", err)
	}
	if !info.Capabilities["zstd_decompress"] {
		t.Fatalf("probe reported zstd_decompress=false")
	}
}

func TestListSupportedIncludesImplementedCompressionAndCorruptionTests(t *testing.T) {
	var buf bytes.Buffer
	listSupportedTests(&buf)

	var tests []string
	if err := json.Unmarshal(buf.Bytes(), &tests); err != nil {
		t.Fatalf("decode list output: %v", err)
	}

	seen := make(map[string]struct{}, len(tests))
	for _, test := range tests {
		seen[test] = struct{}{}
	}
	for _, want := range []string{"journal-zstd-compressed-read", "journal-corruption-append-resilient", "journal-verify-corruption-detection"} {
		if _, ok := seen[want]; !ok {
			t.Fatalf("supported test list missing %q: %v", want, tests)
		}
	}
}

func TestUIDFromFilenameConformanceCaseIsReal(t *testing.T) {
	result := testUIDFromFilename()
	if result.Status != "PASS" || result.Actual != true {
		t.Fatalf("testUIDFromFilename result = %#v, want PASS true", result)
	}

	uid, hasUID, errCode := parseUIDFromJournalFilename("user-1000.journal")
	if uid != 1000 || !hasUID || errCode != "" {
		t.Fatalf("user-1000.journal parsed as uid=%d hasUID=%v err=%q", uid, hasUID, errCode)
	}
	_, _, errCode = parseUIDFromJournalFilename("user-foo.journal")
	if errCode != "EINVAL" {
		t.Fatalf("user-foo.journal err=%q, want EINVAL", errCode)
	}
	_, _, errCode = parseUIDFromJournalFilename("user-65535.journal")
	if errCode != "ENXIO" {
		t.Fatalf("user-65535.journal err=%q, want ENXIO", errCode)
	}
	_, _, errCode = parseUIDFromJournalFilename("user@0000000000000000-0000000000000000.journal~")
	if errCode != "EREMOTE" {
		t.Fatalf("user@*.journal~ err=%q, want EREMOTE", errCode)
	}
}

func TestDefaultFixtureBaseFindsRepositoryRoot(t *testing.T) {
	base := defaultFixtureBase()
	if _, err := os.Stat(filepath.Join(base, "tests", "conformance", "manifests", "conformance-v01.json")); err != nil {
		t.Fatalf("defaultFixtureBase() = %q, manifest stat error: %v", base, err)
	}
}
