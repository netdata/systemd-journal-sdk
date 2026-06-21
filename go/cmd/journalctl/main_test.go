package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const validFSSVerificationKey = "c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900"

var (
	cliMachineID = journal.UUID{0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f}
	cliBootID    = journal.UUID{0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f}
)

func cliOptions() journal.Options {
	return journal.Options{MachineID: cliMachineID, BootID: cliBootID}
}

func cliEntryOptions(monotonic uint64) journal.EntryOptions {
	return journal.EntryOptions{
		RealtimeUsec:     1_700_000_000_000_000 + monotonic,
		RealtimeUsecSet:  true,
		MonotonicUsec:    monotonic,
		MonotonicUsecSet: true,
	}
}

func TestRunTailReturnsLatestEntries(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{
		{message: "entry-1", priority: "6"},
		{message: "entry-2", priority: "6"},
		{message: "entry-3", priority: "6"},
		{message: "entry-4", priority: "6"},
		{message: "entry-5", priority: "6"},
	})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "--tail", "2", "--output=cat"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --tail error: %v; stderr=%s", err, stderr.String())
	}

	if got, want := stdout.String(), "entry-4\nentry-5\n"; got != want {
		t.Fatalf("--tail output = %q, want %q", got, want)
	}
}

func TestShowTailLimitZeroDoesNotScan(t *testing.T) {
	j := &trackingCLIJournal{}
	var stdout bytes.Buffer

	if err := showTail(j, 0, nil, nil, &stdout, false, false, &cliPostFilters{}, cursorControl{}, outputOptions{mode: "short"}); err != nil {
		t.Fatalf("showTail limit=0 error: %v", err)
	}
	if got, want := stdout.String(), "-- No entries --\n"; got != want {
		t.Fatalf("showTail limit=0 output = %q, want %q", got, want)
	}
	if j.seekHeadCalls != 0 || j.seekTailCalls != 0 || j.nextCalls != 0 || j.previousCalls != 0 || j.getEntryCalls != 0 {
		t.Fatalf("showTail limit=0 scanned journal: seekHead=%d seekTail=%d next=%d previous=%d getEntry=%d",
			j.seekHeadCalls, j.seekTailCalls, j.nextCalls, j.previousCalls, j.getEntryCalls)
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
	w, err := journal.Create(path, cliOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "binary export"),
		{Name: "BINARY", Value: []byte{0x00, 0x01, '\n', 0xff}},
	}, cliEntryOptions(1)); err != nil {
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

func TestRunFieldFlag(t *testing.T) {
	path := writeCLIJournal(t, []cliEntry{
		{message: "alpha", priority: "3"},
		{message: "beta", priority: "6"},
	})

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--file", path, "-F", "PRIORITY"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run -F error: %v; stderr=%s", err, stderr.String())
	}
	values := stdout.String()
	for _, want := range []string{"3\n", "6\n"} {
		if !strings.Contains(values, want) {
			t.Fatalf("-F output missing %q:\n%s", want, values)
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

type trackingCLIJournal struct {
	seekHeadCalls int
	seekTailCalls int
	nextCalls     int
	previousCalls int
	getEntryCalls int
}

func (j *trackingCLIJournal) Close() error    { return nil }
func (j *trackingCLIJournal) AddMatch([]byte) {}
func (j *trackingCLIJournal) AddDisjunction() {}
func (j *trackingCLIJournal) AddConjunction() {}
func (j *trackingCLIJournal) FlushMatches()   {}
func (j *trackingCLIJournal) SeekHead() error {
	j.seekHeadCalls++
	return nil
}
func (j *trackingCLIJournal) SeekTail() error {
	j.seekTailCalls++
	return nil
}
func (j *trackingCLIJournal) SeekRealtimeUsec(uint64) error   { return nil }
func (j *trackingCLIJournal) SeekCursor(string) error         { return nil }
func (j *trackingCLIJournal) TestCursor(string) (bool, error) { return false, nil }
func (j *trackingCLIJournal) Next() (int, error) {
	j.nextCalls++
	return 0, nil
}
func (j *trackingCLIJournal) Previous() (int, error) {
	j.previousCalls++
	return 0, nil
}
func (j *trackingCLIJournal) GetEntry() (*journal.Entry, error) {
	j.getEntryCalls++
	return nil, nil
}
func (j *trackingCLIJournal) SetOutputMode(string)                         {}
func (j *trackingCLIJournal) ProcessOutput(*journal.Entry) (string, error) { return "", nil }
func (j *trackingCLIJournal) ListBoots() ([]journal.BootInfo, error)       { return nil, nil }
func (j *trackingCLIJournal) EnumerateFields() ([]string, error)           { return nil, nil }
func (j *trackingCLIJournal) VisitUnique(string, func([]byte) error) error { return nil }

func writeCLIJournal(t *testing.T, entries []cliEntry) string {
	t.Helper()

	path := filepath.Join(t.TempDir(), "cli.journal")
	w, err := journal.Create(path, cliOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for i, entry := range entries {
		if err := w.Append([]journal.Field{
			journal.StringField("MESSAGE", entry.message),
			journal.StringField("PRIORITY", entry.priority),
		}, journal.EntryOptions{RealtimeUsec: uint64(1000 + i), MonotonicUsec: uint64(i + 1)}); err != nil {
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
	if err := run([]string{"--verify", "--directory", t.TempDir()}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("expected empty verify directory to succeed: %v; stderr=%q", err, stderr.String())
	}
	if stdout.Len() != 0 {
		t.Fatalf("expected no stdout, got: %q", stdout.String())
	}
	if stderr.Len() != 0 {
		t.Fatalf("expected no stderr, got: %q", stderr.String())
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
	opts := cliOptions()
	opts.Seal = &journal.SealOptions{Seed: seed, IntervalUsec: 1000000, StartUsec: 1000000}
	w, err := journal.Create(path, opts)
	if err != nil {
		t.Fatalf("Create sealed error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "sealed-ok"),
		journal.StringField("PRIORITY", "6"),
	}, journal.EntryOptions{RealtimeUsec: 1500000, MonotonicUsec: 1}); err != nil {
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
	opts := cliOptions()
	opts.Seal = &journal.SealOptions{Seed: seed, IntervalUsec: 1000000, StartUsec: 1000000}
	w, err := journal.Create(path, opts)
	if err != nil {
		t.Fatalf("Create sealed error: %v", err)
	}
	if err := w.Append([]journal.Field{
		journal.StringField("MESSAGE", "sealed-wrong"),
	}, journal.EntryOptions{RealtimeUsec: 1500000, MonotonicUsec: 1}); err != nil {
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

// -- SOW-0121 parser parity tests ----------------------------------
//
// Every official systemd v260.1 long option must be recognized by the
// parser. The set is enumerated by the shared manifest at
// tests/parser-parity/v260-manifest.json and is duplicated here so the
// parser contract is enforced by Go unit tests in addition to the
// shared Python harness and the Rust unit tests.

var officialLongOptions = []string{
	"system", "user", "machine", "merge", "directory", "file", "root", "image",
	"image-policy", "namespace", "since", "until", "cursor", "after-cursor",
	"cursor-file", "boot", "this-boot", "unit", "user-unit", "invocation",
	"identifier", "exclude-identifier", "priority", "facility", "grep",
	"case-sensitive", "dmesg", "output", "output-fields", "lines", "reverse",
	"show-cursor", "utc", "catalog", "no-hostname", "no-full", "full", "all",
	"follow", "no-tail", "truncate-newline", "quiet", "synchronize-on-exit",
	"no-pager", "pager-end", "verify-key", "interval", "force", "setup-keys",
	"help", "version", "new-id128", "fields", "field", "list-boots",
	"list-invocations", "list-namespaces", "disk-usage", "vacuum-size",
	"vacuum-files", "vacuum-time", "verify", "sync", "relinquish-var",
	"smart-relinquish-var", "flush", "rotate", "header", "list-catalog",
	"dump-catalog", "update-catalog",
}

var officialOutputModes = []string{
	"short", "short-full", "short-iso", "short-iso-precise", "short-precise",
	"short-monotonic", "short-delta", "short-unix", "verbose", "export", "json",
	"json-pretty", "json-sse", "json-seq", "cat", "with-unit",
}

func TestEveryOfficialLongOptionIsParsed(t *testing.T) {
	for _, opt := range officialLongOptions {
		// boolean-style options get a `=true` placeholder so the flag
		// package does not try to consume the next positional.
		argv := []string{"--" + opt, "true"}
		err := parseOnlyForTest(argv)
		if err != nil {
			// flag.ErrHelp is a normal parser exit for --help; the
			// parser still recognized the option.
			if errors.Is(err, flag.ErrHelp) {
				continue
			}
			t.Errorf("parser rejected official option --%s: %v", opt, err)
		}
	}
}

func TestEveryOfficialOutputModeIsAccepted(t *testing.T) {
	for _, mode := range officialOutputModes {
		argv := []string{"--output=" + mode}
		if err := parseOnlyForTest(argv); err != nil {
			t.Errorf("parser rejected output mode %q: %v", mode, err)
		}
	}
}

func TestParseLinesLimitValuePreservesSystemdDirection(t *testing.T) {
	tests := []struct {
		name  string
		value string
		want  linesLimit
	}{
		{name: "default value", value: "10", want: linesLimit{set: true, count: 10}},
		{name: "tail", value: "25", want: linesLimit{set: true, count: 25}},
		{name: "oldest", value: "+25", want: linesLimit{set: true, oldest: true, count: 25}},
		{name: "all", value: "all", want: linesLimit{set: true, all: true}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := parseLinesLimitValue(tt.value)
			if err != nil {
				t.Fatalf("parseLinesLimitValue(%q) error: %v", tt.value, err)
			}
			if got != tt.want {
				t.Fatalf("parseLinesLimitValue(%q) = %+v, want %+v", tt.value, got, tt.want)
			}
		})
	}
	if _, err := parseLinesLimitValue("not-a-number"); err == nil {
		t.Fatal("parseLinesLimitValue accepted invalid value")
	}
	if _, err := parseLinesLimitValue(""); err == nil {
		t.Fatal("parseLinesLimitValue accepted explicit empty value")
	}
	if _, err := parseLinesLimitValue("-1"); err == nil {
		t.Fatal("parseLinesLimitValue accepted negative value")
	}
}

func TestOptionalArgumentPreprocessingMatchesSystemd(t *testing.T) {
	fs, _ := newCLIFlagSet(io.Discard)
	tests := []struct {
		name string
		args []string
		want []string
	}{
		{
			name: "bare boot",
			args: []string{"--boot", "TEST_ID=value"},
			want: []string{"--boot=0", "TEST_ID=value"},
		},
		{
			name: "explicit empty boot",
			args: []string{"--boot="},
			want: []string{"--boot="},
		},
		{
			name: "short attached boot",
			args: []string{"-ball"},
			want: []string{"-b=all"},
		},
		{
			name: "short equals boot keeps equals in value",
			args: []string{"-b=true"},
			want: []string{"-b==true"},
		},
		{
			name: "bare lines",
			args: []string{"--lines", "TEST_ID=value"},
			want: []string{"--lines=10", "TEST_ID=value"},
		},
		{
			name: "short attached lines",
			args: []string{"-n5"},
			want: []string{"-n=5"},
		},
		{
			name: "short equals lines keeps equals in value",
			args: []string{"-n=5"},
			want: []string{"-n==5"},
		},
		{
			name: "short cluster with value option",
			args: []string{"-qrn2"},
			want: []string{"-q", "-r", "-n=2"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := preprocessOptionalArgs(normalizeShortFlags(tt.args, fs))
			if fmt.Sprint(got) != fmt.Sprint(tt.want) {
				t.Fatalf("preprocess = %#v, want %#v", got, tt.want)
			}
		})
	}
}

func TestPortableUnsupportedMessageFormat(t *testing.T) {
	unsupported := []string{
		"--sync", "--flush", "--rotate", "--relinquish-var",
		"--smart-relinquish-var", "--list-namespaces", "--list-catalog",
		"--dump-catalog", "--update-catalog",
	}
	for _, opt := range unsupported {
		fs, flags := newCLIFlagSet(io.Discard)
		if err := fs.Parse([]string{opt}); err != nil {
			t.Fatalf("parse error for %s: %v", opt, err)
		}
		err := flags.validate()
		if err == nil {
			t.Errorf("expected non-nil error for %s", opt)
			continue
		}
		if !strings.Contains(err.Error(), "portable mode does not support") {
			t.Errorf("expected portable message for %s, got: %v", opt, err)
		}
	}
}

func TestPortableUnsupportedForSourceOptions(t *testing.T) {
	unsupported := []string{"--machine", "--root", "--image", "--namespace"}
	for _, opt := range unsupported {
		fs, flags := newCLIFlagSet(io.Discard)
		if err := fs.Parse([]string{opt, "/dev/null"}); err != nil {
			t.Fatalf("parse error for %s: %v", opt, err)
		}
		err := flags.validate()
		if err == nil {
			t.Errorf("expected non-nil error for %s", opt)
			continue
		}
		if !strings.Contains(err.Error(), "portable mode does not support") {
			t.Errorf("expected portable message for %s, got: %v", opt, err)
		}
	}
}

func TestSourceExclusivityEnforced(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--directory=/tmp", "--file=/tmp/x.journal"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --directory + --file")
	}
	if !strings.Contains(err.Error(), "at most one of") ||
		!strings.Contains(err.Error(), "--directory") ||
		!strings.Contains(err.Error(), "--file") {
		t.Fatalf("expected source exclusivity error, got: %v", err)
	}
}

func TestResolveFileInputsDeduplicatesRepeatedPaths(t *testing.T) {
	path := filepath.Join(t.TempDir(), "repeated.journal")
	if err := os.WriteFile(path, []byte("not-a-real-journal"), 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	got, err := resolveFileInputs([]string{path, path})
	if err != nil {
		t.Fatalf("resolveFileInputs error: %v", err)
	}
	if len(got) != 1 || got[0] != path {
		t.Fatalf("resolveFileInputs returned %#v, want one copy of %q", got, path)
	}
}

func TestSinceUntilOrderEnforced(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{
		"--file=/tmp/x.journal",
		"--since=2020-01-02",
		"--until=2020-01-01",
	}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --since later than --until")
	}
	if !strings.Contains(err.Error(), "--since= must be before --until=") {
		t.Fatalf("expected since/until order error, got: %v", err)
	}
}

func TestFollowReverseConflictEnforced(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--file=/tmp/x.journal", "--follow", "--reverse"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --follow + --reverse")
	}
	if !strings.Contains(err.Error(), "either --reverse or --follow, not both") {
		t.Fatalf("expected follow/reverse conflict, got: %v", err)
	}
}

func TestOldestLinesConflictAppliesOnlyToShowAction(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--file=/tmp/x.journal", "--lines=+1", "--reverse"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	if err := flags.validateParserInteractions(); err == nil {
		t.Fatalf("expected show action to reject --lines=+N with --reverse")
	}

	fs, flags = newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--file=/tmp/x.journal", "--list-boots", "--lines=+1", "--reverse"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	if err := flags.validateParserInteractions(); err != nil {
		t.Fatalf("non-show action should allow --lines=+N with --reverse, got: %v", err)
	}
}

func TestTimestampParserAcceptsStockISOTForms(t *testing.T) {
	localSpace, err := parseTimestampUsec("2023-11-15 00:00")
	if err != nil {
		t.Fatalf("space local parse error: %v", err)
	}
	localT, err := parseTimestampUsec("2023-11-15T00:00")
	if err != nil {
		t.Fatalf("T local parse error: %v", err)
	}
	if localT != localSpace {
		t.Fatalf("T local = %d, want %d", localT, localSpace)
	}
	for _, value := range []string{
		"2023-11-15T00:00:00Z",
		"2023-11-15T00:00:00.000001Z",
		"2023-11-15T00:00:00+02:00",
		"2023-11-15T00:00+02:00",
	} {
		if _, err := parseTimestampUsec(value); err != nil {
			t.Fatalf("expected %s to parse: %v", value, err)
		}
	}
	if _, err := parseTimestampUsec("2023-11-15T00:00:00+0200"); err == nil {
		t.Fatalf("stock rejects compact timezone offsets")
	}
}

func TestDirectoryInputDoesNotOpenRegularFileAsFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "regular.journal")
	if err := os.WriteFile(path, []byte("not-a-real-journal"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	var stdout, stderr bytes.Buffer
	err := run([]string{"--directory", path, "--output=cat"}, strings.NewReader(""), &stdout, &stderr)
	if err == nil {
		t.Fatalf("expected --directory regular file to fail")
	}
	if !strings.Contains(strings.ToLower(err.Error()), "not a directory") {
		t.Fatalf("expected not-a-directory error, got: %v", err)
	}
}

func TestSynchronizeOnExitTrueIsRejected(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--synchronize-on-exit=true"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --synchronize-on-exit=true")
	}
	if !strings.Contains(err.Error(), "portable mode does not support --synchronize-on-exit") {
		t.Fatalf("expected portable message, got: %v", err)
	}
}

func TestSynchronizeOnExitFalseIsAccepted(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--synchronize-on-exit=false"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	if err := flags.validate(); err != nil {
		t.Fatalf("expected success for --synchronize-on-exit=false, got: %v", err)
	}
}

func TestVacuumWithoutDirectoryIsRejected(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--vacuum-size=1G"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --vacuum-size without --directory")
	}
	if !strings.Contains(err.Error(), "portable mode does not support --vacuum-*") {
		t.Fatalf("expected portable message, got: %v", err)
	}
}

func TestRotateWithVacuumIsDistinctUnsupportedAction(t *testing.T) {
	fs, flags := newCLIFlagSet(io.Discard)
	if err := fs.Parse([]string{"--rotate", "--vacuum-files=2"}); err != nil {
		t.Fatalf("parse error: %v", err)
	}
	err := flags.validate()
	if err == nil {
		t.Fatalf("expected error for --rotate with --vacuum-files")
	}
	if !strings.Contains(err.Error(), "portable mode does not support --rotate with --vacuum-*") {
		t.Fatalf("expected rotate-and-vacuum portable message, got: %v", err)
	}
}

func TestRunVacuumFilesWithDirectoryDeletesOldestArchived(t *testing.T) {
	source := writeCLIJournal(t, []cliEntry{{message: "vacuum", priority: "6"}})
	dir := t.TempDir()
	active := filepath.Join(dir, "system.journal")
	copyFile(t, source, active)

	names := []string{
		archivedJournalNameForTest(1, 1_700_004_100_000_000),
		archivedJournalNameForTest(2, 1_700_004_100_000_500),
		archivedJournalNameForTest(3, 1_700_004_100_001_000),
	}
	for _, name := range names {
		copyFile(t, source, filepath.Join(dir, name))
	}

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--directory", dir, "--vacuum-files=2", "--quiet"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --vacuum-files error: %v; stderr=%s", err, stderr.String())
	}

	if _, err := os.Stat(active); err != nil {
		t.Fatalf("active journal was not protected: %v", err)
	}
	for _, removed := range names[:2] {
		if _, err := os.Stat(filepath.Join(dir, removed)); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("old archived journal %s exists after vacuum: %v", removed, err)
		}
	}
	if _, err := os.Stat(filepath.Join(dir, names[2])); err != nil {
		t.Fatalf("newest archived journal was not retained: %v", err)
	}
}

func TestRunVacuumTimeZeroIsNoop(t *testing.T) {
	source := writeCLIJournal(t, []cliEntry{{message: "vacuum", priority: "6"}})
	dir := t.TempDir()
	name := archivedJournalNameForTest(1, 1_700_004_100_000_000)
	copyFile(t, source, filepath.Join(dir, name))

	var stdout, stderr bytes.Buffer
	if err := run([]string{"--directory", dir, "--vacuum-time=0s", "--quiet"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --vacuum-time=0s error: %v; stderr=%s", err, stderr.String())
	}
	if _, err := os.Stat(filepath.Join(dir, name)); err != nil {
		t.Fatalf("archived journal should remain after zero-time vacuum: %v", err)
	}
}

func TestVersionPrintsBaselineMetadata(t *testing.T) {
	var stdout, stderr bytes.Buffer
	if err := run([]string{"--version"}, strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run --version error: %v; stderr=%s", err, stderr.String())
	}
	banner := stdout.String()
	if !strings.Contains(banner, "v260.1") || !strings.Contains(banner, "baseline") {
		t.Fatalf("expected version banner, got: %q", banner)
	}
}

func TestBootMergeConflictEnforced(t *testing.T) {
	var stdout, stderr bytes.Buffer
	err := run([]string{"--file=/tmp/x.journal", "--boot", "--merge"}, strings.NewReader(""), &stdout, &stderr)
	if err == nil {
		t.Fatalf("expected error for --boot + --merge")
	}
	if !strings.Contains(err.Error(), "--boot or --list-boots with --merge is not supported") {
		t.Fatalf("expected boot/merge conflict, got: %v", err)
	}
}

func TestBootAllMergeIsNotRejectedAsConflict(t *testing.T) {
	var stdout, stderr bytes.Buffer
	err := run([]string{"--file=/tmp/x.journal", "--boot=all", "--merge"}, strings.NewReader(""), &stdout, &stderr)
	if err == nil {
		t.Fatalf("expected invalid fixture to fail")
	}
	if strings.Contains(err.Error(), "--boot or --list-boots with --merge is not supported") {
		t.Fatalf("did not expect boot=all/merge conflict, got: %v", err)
	}
}

func TestUnrecognizedOptionIsRejected(t *testing.T) {
	fs, _ := newCLIFlagSet(io.Discard)
	err := fs.Parse([]string{"--not-an-official-option"})
	if err == nil {
		t.Fatalf("expected parse error for unknown option")
	}
	if !strings.Contains(err.Error(), "flag provided but not defined") {
		t.Fatalf("expected unknown flag error, got: %v", err)
	}
}

// parseOnlyForTest parses argv through the flag set used by `run`.
// It returns nil if the parser accepted every token. It does NOT run
// validation or dispatch so unknown-class options can be tested in
// isolation.
func parseOnlyForTest(argv []string) error {
	fs, _ := newCLIFlagSet(io.Discard)
	return fs.Parse(permuteFlagArgs(preprocessOptionalArgs(normalizeShortFlags(argv, fs)), fs))
}

func archivedJournalNameForTest(seq uint64, realtime uint64) string {
	return fmt.Sprintf("system@12121212121212121212121212121212-%016x-%016x.journal", seq, realtime)
}

func copyFile(t *testing.T, src, dst string) {
	t.Helper()
	content, err := os.ReadFile(src) // nosec G304 - test fixture path is generated by the test.
	if err != nil {
		t.Fatalf("ReadFile(%s) error: %v", src, err)
	}
	if err := os.WriteFile(dst, content, 0o644); err != nil {
		t.Fatalf("WriteFile(%s) error: %v", dst, err)
	}
}
