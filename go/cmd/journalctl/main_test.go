package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"path/filepath"
	"strings"
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func TestRunTailReturnsLatestEntries(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{
		{message: "entry-1", priority: "6"},
		{message: "entry-2", priority: "6"},
		{message: "entry-3", priority: "6"},
		{message: "entry-4", priority: "6"},
		{message: "entry-5", priority: "6"},
	})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "--tail", "2"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --tail error: %v; stderr=%s", err, stderr.String())
	}

	if got, want := stdout.String(), "entry-4\nentry-5\n"; got != want {
		t.Fatalf("--tail output = %q, want %q", got, want)
	}
}

func TestRunMatchSemanticsAndStandaloneDisjunction(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{
		{message: "alpha", priority: "3"},
		{message: "beta", priority: "6"},
		{message: "gamma", priority: "3"},
	})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "--output=json", "PRIORITY=3", "+", "MESSAGE=beta"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run disjunction error: %v; stderr=%s", err, stderr.String())
	}
	rows := decodeJSONLines(t, stdout.String())
	if len(rows) != 3 {
		t.Fatalf("disjunction returned %d rows, want 3; rows=%v", len(rows), rows)
	}

	stdout.Reset()
	stderr.Reset()
	if err := run([]string{"--file", path, "--output=json", "PRIORITY=3", "MESSAGE=beta"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run conjunction error: %v; stderr=%s", err, stderr.String())
	}
	rows = decodeJSONLines(t, stdout.String())
	if len(rows) != 0 {
		t.Fatalf("conjunction returned %d rows, want 0; rows=%v", len(rows), rows)
	}

	stdout.Reset()
	stderr.Reset()
	if err := run([]string{"--file", path, "--output=json", "PRIORITY=3", "PRIORITY=6"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run same-field OR error: %v; stderr=%s", err, stderr.String())
	}
	rows = decodeJSONLines(t, stdout.String())
	if len(rows) != 3 {
		t.Fatalf("same-field OR returned %d rows, want 3; rows=%v", len(rows), rows)
	}
}

func TestRunExportOutputUsesBinaryEncoding(t *testing.T) {
	path := filepath.Join(t.TempDir(), "export.journal")
	w, err := journal.Create(path, journal.Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "binary export"),
		{Name: "BINARY", Value: []byte{0x00, 0x01, '\n', 0xff}},
	}, journal.EntryOptions{}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "--output=export", "--head", "1"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run export error: %v; stderr=%s", err, stderr.String())
	}

	raw := stdout.Bytes()
	idx := bytes.Index(raw, []byte("BINARY\n"))
	if idx < 0 {
		t.Fatalf("export output missing binary field header:\n%s", raw)
	}
	sizeStart := idx + len("BINARY\n")
	if len(raw) < sizeStart+8 {
		t.Fatalf("export output missing binary size prefix")
	}
	size := binary.LittleEndian.Uint64(raw[sizeStart : sizeStart+8])
	if size != 4 {
		t.Fatalf("binary export size = %d, want 4", size)
	}
	payload := raw[sizeStart+8 : sizeStart+8+int(size)]
	if !bytes.Equal(payload, []byte{0x00, 0x01, '\n', 0xff}) {
		t.Fatalf("binary export payload = %v, want [0 1 10 255]", payload)
	}
}

func TestRunFieldsFlag(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "alpha", priority: "3"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "--fields"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --fields error: %v; stderr=%s", err, stderr.String())
	}
	fields := stdout.String()
	for _, want := range []string{"MESSAGE\n", "PRIORITY\n"} {
		if !strings.Contains(fields, want) {
			t.Fatalf("--fields output missing %q:\n%s", want, fields)
		}
	}
}

func TestRunRejectsInventedPlusPrefixSyntax(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "alpha", priority: "3"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "+MESSAGE=alpha+MESSAGE=beta"}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("run accepted non-systemd +FIELD=value+FIELD=value syntax")
	}
}

type cliEntry struct {
	message  string
	priority string
}

func writeCLIJournal(t *testing.T, entries []cliEntry) string {
	t.Helper()

	path := filepath.Join(t.TempDir(), "cli.journal")
	w, err := journal.Create(path, journal.Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for i, entry := range entries {
		if err := w.Append([]journal.Field{
			journal.StringField("MESSAGE", entry.message),
			journal.StringField("PRIORITY", entry.priority),
		}, journal.EntryOptions{RealtimeUsec: uint64(1000 + i)}); err != nil {
			t.Fatalf("Append %q error: %v", entry.message, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}
	return path
}

func decodeJSONLines(t *testing.T, raw string) []map[string]interface{} {
	t.Helper()

	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}

	var rows []map[string]interface{}
	for _, line := range strings.Split(raw, "\n") {
		var row map[string]interface{}
		if err := json.Unmarshal([]byte(line), &row); err != nil {
			t.Fatalf("decode JSON line %q: %v", line, err)
		}
		rows = append(rows, row)
	}
	return rows
}
