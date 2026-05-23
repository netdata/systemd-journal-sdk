package journal

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestLogRotatesByEntryCountAndJournalctlDirectory(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(3),
	})

	for i := 0; i < 7; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("rotation-%d", i)),
			StringField("TEST_ID", "directory-rotation"),
			StringField("PRIORITY", "6"),
		}, EntryOptions{RealtimeUsec: 1_700_002_000_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 3 {
		t.Fatalf("journal file count = %d, want 3; files=%v", len(files), files)
	}
	wantHeads := []uint64{1, 4, 7}
	for i, path := range files {
		if base := filepath.Base(path); !strings.HasPrefix(base, "system@") || !strings.HasSuffix(base, ".journal") {
			t.Fatalf("journal file name = %q, want archived system@*.journal", base)
		}
		verifyJournalctl(t, path)
		snapshot := readJournalSnapshot(t, path)
		if snapshot.header.state != stateArchived {
			t.Fatalf("%s state = %d, want archived", path, snapshot.header.state)
		}
		if snapshot.header.headEntrySeqnum != wantHeads[i] {
			t.Fatalf("%s head seqnum = %d, want %d", path, snapshot.header.headEntrySeqnum, wantHeads[i])
		}
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-rotation")
	if len(rows) != 7 {
		t.Fatalf("directory row count = %d, want 7", len(rows))
	}
}

func TestLogActiveFileJournalctlDirectoryReadback(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "active directory readback"),
		StringField("TEST_ID", "directory-active"),
	}, EntryOptions{RealtimeUsec: 1_700_002_050_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(active) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-active")
	if len(rows) != 1 {
		t.Fatalf("active directory row count = %d, want 1", len(rows))
	}
	snapshot := readJournalSnapshot(t, log.ActivePath())
	if snapshot.header.state != stateOnline {
		t.Fatalf("active state = %d, want online", snapshot.header.state)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogCloseWithoutAppendDoesNotCreateFile(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	activePath := log.ActivePath()

	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	if _, err := os.Stat(activePath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("active file stat error = %v, want not exist", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(second) error = %v, want nil", err)
	}
}

func TestLogCloseRemovesEmptyActiveFile(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	activePath := log.ActivePath()
	if err := log.ensureWriter(); err != nil {
		t.Fatalf("ensureWriter() error = %v", err)
	}
	if _, err := os.Stat(activePath); err != nil {
		t.Fatalf("active file stat error = %v", err)
	}

	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	if _, err := os.Stat(activePath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("active file stat error = %v, want not exist", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(second) error = %v, want nil", err)
	}
}

func TestLogRotatesByFileSize(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxFileSize(8 * 1024),
	})

	for i := 0; i < 20; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("size-rotation-%02d-%s", i, strings.Repeat("x", 1000))),
			StringField("TEST_ID", "directory-size-rotation"),
		}, EntryOptions{RealtimeUsec: 1_700_002_075_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) < 2 {
		t.Fatalf("journal file count = %d, want at least 2 for size rotation", len(files))
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-size-rotation")
	if len(rows) != 20 {
		t.Fatalf("size-rotation row count = %d, want 20", len(rows))
	}
}

func TestLogRetainsByFileCount(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(2),
	})

	for i := 0; i < 5; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("retained-%d", i)),
			StringField("TEST_ID", "directory-retention-count"),
		}, EntryOptions{RealtimeUsec: 1_700_002_100_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal file count after retention = %d, want 2; files=%v", len(files), files)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-retention-count")
	if len(rows) != 2 {
		t.Fatalf("retained row count = %d, want 2", len(rows))
	}
	assertJSONField(t, rows[0], "MESSAGE", "retained-3")
	assertJSONField(t, rows[1], "MESSAGE", "retained-4")
}

func TestLogRetainsArchivedFilesWhileActiveFileSurvives(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(1),
	})

	for i := 0; i < 3; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("active-retained-%d", i)),
			StringField("TEST_ID", "directory-active-retention"),
		}, EntryOptions{RealtimeUsec: 1_700_002_150_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal file count before close = %d, want 2; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, log.ActivePath())
	if snapshot.header.state != stateOnline {
		t.Fatalf("active state = %d, want online", snapshot.header.state)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-active-retention")
	if len(rows) != 2 {
		t.Fatalf("active-retention row count = %d, want 2", len(rows))
	}
	assertJSONField(t, rows[0], "MESSAGE", "active-retained-1")
	assertJSONField(t, rows[1], "MESSAGE", "active-retained-2")

	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogRetainsByTotalBytes(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxBytes(1),
	})

	for i := 0; i < 3; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("byte-retention-%d", i)),
			StringField("TEST_ID", "directory-retention-bytes"),
		}, EntryOptions{RealtimeUsec: 1_700_002_200_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 0 {
		t.Fatalf("journal files after byte retention = %d, want 0; files=%v", len(files), files)
	}
}

func TestLogReopensActiveFileAndContinuesSequence(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(3),
	}
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 2; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("reopen-%d", i)),
			StringField("TEST_ID", "directory-reopen"),
		}, EntryOptions{RealtimeUsec: 1_700_002_250_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync(first) error = %v", err)
	}
	activePath := log.ActivePath()
	if err := log.writer.Close(); err != nil {
		t.Fatalf("writer.Close(first active) error = %v", err)
	}
	log.writer = nil
	log.closed = true

	reopened, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(reopen) error = %v", err)
	}
	if reopened.ActivePath() != activePath {
		t.Fatalf("ActivePath after reopen = %q, want %q", reopened.ActivePath(), activePath)
	}
	for i := 2; i < 4; i++ {
		if err := reopened.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("reopen-%d", i)),
			StringField("TEST_ID", "directory-reopen"),
		}, EntryOptions{RealtimeUsec: 1_700_002_250_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(reopen %d) error = %v", i, err)
		}
	}
	if err := reopened.Close(); err != nil {
		t.Fatalf("Close(reopen) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal file count after reopen = %d, want 2; files=%v", len(files), files)
	}
	wantHeads := []uint64{1, 4}
	for i, path := range files {
		verifyJournalctl(t, path)
		snapshot := readJournalSnapshot(t, path)
		if snapshot.header.headEntrySeqnum != wantHeads[i] {
			t.Fatalf("%s head seqnum = %d, want %d", path, snapshot.header.headEntrySeqnum, wantHeads[i])
		}
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-reopen")
	if len(rows) != 4 {
		t.Fatalf("reopen row count = %d, want 4", len(rows))
	}
	for i := 0; i < 4; i++ {
		assertJSONField(t, rows[i], "MESSAGE", fmt.Sprintf("reopen-%d", i))
	}
}

func TestNewLogClosesReopenedWriterOnRetentionFailure(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options: testOptions(),
		Source:  "system",
	}

	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "reopen cleanup"),
		StringField("TEST_ID", "newlog-retention-failure"),
	}, EntryOptions{RealtimeUsec: 1_700_002_275_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync(first) error = %v", err)
	}
	if err := log.writer.Close(); err != nil {
		t.Fatalf("writer.Close(first active) error = %v", err)
	}
	log.writer = nil
	log.closed = true

	syntheticErr := errors.New("synthetic directory sync failure")
	oldSync := syncJournalDirectory
	syncJournalDirectory = func(string) error {
		return syntheticErr
	}
	_, err = NewLog(root, config)
	syncJournalDirectory = oldSync
	if !errors.Is(err, syntheticErr) {
		t.Fatalf("NewLog(reopen with retention failure) error = %v, want %v", err, syntheticErr)
	}

	reopened, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(after failed reopen) error = %v", err)
	}
	if err := reopened.Close(); err != nil {
		t.Fatalf("Close(after failed reopen) error = %v", err)
	}
}

func TestLogBinaryFieldCompatibility(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	value := []byte{0x00, 0x01, 'x', 0x80, 0xff}
	if err := log.Append([]Field{
		StringField("MESSAGE", "directory binary"),
		StringField("TEST_ID", "directory-binary"),
		{Name: "BINARY_PAYLOAD", Value: value},
	}, EntryOptions{RealtimeUsec: 1_700_002_300_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(binary) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-binary")
	if len(rows) != 1 {
		t.Fatalf("directory binary row count = %d, want 1", len(rows))
	}
	assertJSONByteArray(t, rows[0], "BINARY_PAYLOAD", value)
}

func TestLogCustomSourcePrefix(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "netdata-plugin",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "custom source"),
		StringField("TEST_ID", "directory-custom-source"),
	}, EntryOptions{RealtimeUsec: 1_700_002_400_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(custom source) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("custom source file count = %d, want 1; files=%v", len(files), files)
	}
	if base := filepath.Base(files[0]); !strings.HasPrefix(base, "netdata-plugin@") {
		t.Fatalf("custom source filename = %q, want netdata-plugin@*.journal", base)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-custom-source")
	if len(rows) != 1 {
		t.Fatalf("custom source directory row count = %d, want 1", len(rows))
	}
}

func TestLogAppendAfterCloseReturnsClosedError(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "close then append"),
	}, EntryOptions{RealtimeUsec: 1_700_002_500_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(before close) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	err := log.Append([]Field{
		StringField("MESSAGE", "should fail"),
	}, EntryOptions{})
	if err != errWriterClosed {
		t.Fatalf("Append(after close) error = %v, want %v", err, errWriterClosed)
	}
}

func TestLogCloseIsIdempotentAfterArchiveCleanupFailure(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "archive cleanup failure"),
		StringField("TEST_ID", "close-cleanup-failure"),
	}, EntryOptions{RealtimeUsec: 1_700_002_600_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(before close) error = %v", err)
	}

	syntheticErr := errors.New("synthetic archive sync failure")
	oldSync := syncJournalDirectory
	syncJournalDirectory = func(string) error {
		return syntheticErr
	}
	err := log.Close()
	syncJournalDirectory = oldSync
	if !errors.Is(err, syntheticErr) {
		t.Fatalf("Close(first) error = %v, want %v", err, syntheticErr)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(second) error = %v, want nil", err)
	}
}

func newTestLog(t *testing.T, config LogConfig) (*Log, string) {
	t.Helper()

	root := t.TempDir()
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog() error = %v", err)
	}
	return log, filepath.Join(root, config.Options.MachineID.String())
}

func journalFiles(t *testing.T, dir string) []string {
	t.Helper()

	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("ReadDir(%s) error = %v", dir, err)
	}
	var files []string
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".journal") {
			continue
		}
		files = append(files, filepath.Join(dir, entry.Name()))
	}
	return files
}

func requireJournalctl(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("journalctl"); err != nil {
		t.Skip("journalctl is not installed")
	}
}

func runJournalctlDirectoryJSON(t *testing.T, dir string, matches ...string) []map[string]any {
	t.Helper()

	args := append([]string{"--directory", dir, "--output=json", "--no-pager"}, matches...)
	cmd := exec.Command("journalctl", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("journalctl %s failed: %v\n%s", strings.Join(args, " "), err, output)
	}
	output = bytes.TrimSpace(output)
	if len(output) == 0 {
		return nil
	}

	lines := bytes.Split(output, []byte{'\n'})
	rows := make([]map[string]any, 0, len(lines))
	for _, line := range lines {
		var row map[string]any
		if err := json.Unmarshal(line, &row); err != nil {
			t.Fatalf("json.Unmarshal(%q) error = %v", line, err)
		}
		rows = append(rows, row)
	}
	return rows
}
