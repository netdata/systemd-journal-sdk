package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const validFSSVerificationKey = "c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900"

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

func TestRunVerifyValidFile(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-ok", priority: "6"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify", "--file", path}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify error: %v; stderr=%s", err, stderr.String())
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "PASS:") {
		t.Fatalf("expected PASS in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyOnlyValidFile(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-only-ok", priority: "6"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-only", "--file", path}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify-only error: %v; stderr=%s", err, stderr.String())
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "PASS:") {
		t.Fatalf("expected PASS in stderr, got: %q", stderr.String())
	}
	if strings.Contains(stderr.String(), "verify-only-ok") {
		t.Fatal("--verify-only emitted normal journal output")
	}
}

func TestRunVerifyDirectoryFollowsSymlinkAndSkipsDirectories(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-dir", priority: "6"}})
	dir := t.TempDir()
	if err := os.Symlink(path, filepath.Join(dir, "linked.journal")); err != nil {
		t.Fatalf("symlink journal: %v", err)
	}
	if err := os.Mkdir(filepath.Join(dir, "skip.journal"), 0o755); err != nil {
		t.Fatalf("mkdir skipped journal name: %v", err)
	}

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify", "--directory", dir}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify --directory error: %v; stderr=%s", err, stderr.String())
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if got := strings.Count(stderr.String(), "PASS:"); got != 1 {
		t.Fatalf("expected one PASS in stderr, got %d: %q", got, stderr.String())
	}
	if strings.Contains(stderr.String(), "FAIL:") {
		t.Fatalf("expected no FAIL in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyDirectoryEmpty(t *testing.T) {
	var stdout, stderr bytes.Buffer
	err := run([]string{"--verify", "--directory", t.TempDir()}, strings.NewReader(""), &stdout, &stderr)
	if err == nil {
		t.Fatal("expected error for empty verify directory")
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(err.Error(), "verify: no journal files found") {
		t.Fatalf("expected no journal files error, got err=%v stderr=%q", err, stderr.String())
	}
}

func TestRunVerifyCorruptedFile(t *testing.T) {
	path := filepath.Join("..", "..", "..", "fixtures", "systemd", "test-data", "corrupted", "zstd-truncated-frame.zst")

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify", "--file", path}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("expected error for corrupted file")
	}
	if !strings.Contains(stderr.String(), "FAIL:") {
		t.Fatalf("expected FAIL in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyKeyUnsealedFile(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-key-unsealed", priority: "6"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-key", validFSSVerificationKey, "--file", path}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify-key error: %v; stderr=%s", err, stderr.String())
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "PASS:") {
		t.Fatalf("expected PASS in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyKeyInvalidSeed(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-key-invalid", priority: "6"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-key", "synthetic-test-key", "--file", path}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("expected error for invalid --verify-key seed")
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "Failed to parse seed.") {
		t.Fatalf("expected parse seed error in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyKeyEmptySeed(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "verify-key-empty", priority: "6"}})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-key=", "--file", path}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("expected error for empty --verify-key seed")
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "Failed to parse seed.") {
		t.Fatalf("expected parse seed error in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifySealedWithoutKeyRequiresKey(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{{message: "sealed-without-key", priority: "6"}})
	patchCompatibleFlags(t, path, compatibleSealed)

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify", "--file", path}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("expected error for sealed file without --verify-key")
	}
	if !strings.Contains(stderr.String(), "verification key") {
		t.Fatalf("expected verification key message in stderr, got: %q", stderr.String())
	}
	if strings.Contains(stderr.String(), "PASS:") {
		t.Fatalf("sealed file without key should not pass, got: %q", stderr.String())
	}
}

func TestRunVerifyKeySealedPasses(t *testing.T) {
	path := filepath.Join(t.TempDir(), "cli-sealed.journal")
	seed := make([]byte, 12)
	opts := journal.Options{Seal: &journal.SealOptions{Seed: seed, IntervalUsec: 1000000, StartUsec: 1000000}}
	w, err := journal.Create(path, opts)
	if err != nil {
		t.Fatalf("Create sealed error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "sealed-ok"),
		journal.StringField("PRIORITY", "6"),
	}, journal.EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	key := fmt.Sprintf("%024x/%x-%x", seed, opts.Seal.StartUsec/opts.Seal.IntervalUsec, opts.Seal.IntervalUsec)
	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-key", key, "--file", path}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --verify-key sealed error: %v; stderr=%s", err, stderr.String())
	}
	if !strings.Contains(stderr.String(), "PASS:") {
		t.Fatalf("expected PASS in stderr, got: %q", stderr.String())
	}
}

func TestRunVerifyKeySealedWrongKeyFails(t *testing.T) {
	path := filepath.Join(t.TempDir(), "cli-sealed-wrong.journal")
	seed := make([]byte, 12)
	opts := journal.Options{Seal: &journal.SealOptions{Seed: seed, IntervalUsec: 1000000, StartUsec: 1000000}}
	w, err := journal.Create(path, opts)
	if err != nil {
		t.Fatalf("Create sealed error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "sealed-wrong"),
	}, journal.EntryOptions{RealtimeUsec: 1500000}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--verify-key", "000000000000000000000001/1-f4240", "--file", path}, strings.NewReader(""), &stdout, &stderr); err == nil {
		t.Fatal("expected error for sealed file with wrong key")
	}
	if !strings.Contains(stderr.String(), "FAIL:") {
		t.Fatalf("expected FAIL in stderr, got: %q", stderr.String())
	}
}

func patchCompatibleFlags(t *testing.T, path string, flagsToSet uint32) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatalf("open for patch: %v", err)
	}
	_, err = f.Seek(8, os.SEEK_SET)
	if err != nil {
		t.Fatalf("seek: %v", err)
	}
	var flags uint32
	if err := binary.Read(f, binary.LittleEndian, &flags); err != nil {
		t.Fatalf("read flags: %v", err)
	}
	flags |= flagsToSet
	_, err = f.Seek(8, os.SEEK_SET)
	if err != nil {
		t.Fatalf("seek back: %v", err)
	}
	if err := binary.Write(f, binary.LittleEndian, flags); err != nil {
		t.Fatalf("write flags: %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close patched journal: %v", err)
	}
}
