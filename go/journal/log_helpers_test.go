package journal

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"testing"
)

func closeWriterForTest(t *testing.T, writer *Writer, label string) {
	t.Helper()
	if err := writer.Close(); err != nil {
		t.Fatalf("Close(%s) error = %v", label, err)
	}
}

func assertLifecycleHasCreateRotateDelete(t *testing.T, events []LogLifecycleEvent) {
	t.Helper()
	var created, rotated, deleted bool
	for _, event := range events {
		switch event.Type {
		case LogLifecycleCreated:
			created = event.ActivePath != ""
		case LogLifecycleRotated:
			rotated = event.ArchivedPath != "" && event.ActivePath != ""
		case LogLifecycleDeleted:
			deleted = len(event.DeletedPaths) == 1 && event.DeletedPaths[0] != ""
		}
	}
	if !created || !rotated || !deleted {
		t.Fatalf("events did not include created=%v rotated=%v deleted=%v: %#v", created, rotated, deleted, events)
	}
}

func assertLastLifecycleEventType(t *testing.T, events []LogLifecycleEvent, want LogLifecycleEventType, context string) {
	t.Helper()
	if len(events) == 0 || events[len(events)-1].Type != want {
		t.Fatalf("expected %s lifecycle event %v, got %#v", context, want, events)
	}
}

func assertFirstAndLastLifecycleTypes(t *testing.T, events []LogLifecycleEvent, first LogLifecycleEventType, last LogLifecycleEventType, context string) {
	t.Helper()
	if len(events) < 2 || events[0].Type != first || events[len(events)-1].Type != last {
		t.Fatalf("expected %s events first=%v last=%v, got %#v", context, first, last, events)
	}
}

func mustNewLogForTest(t *testing.T, root string, config LogConfig, label string) *Log {
	t.Helper()
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(%s) error = %v", label, err)
	}
	return log
}

func appendLogRange(t *testing.T, log *Log, prefix string, testID string, start int, end int, realtimeBase uint64) {
	t.Helper()
	for i := start; i < end; i++ {
		appendLogEntry(t, log, fmt.Sprintf("%s-%d", prefix, i), testID, realtimeBase+uint64(i), uint64(i+1))
	}
}

func appendLogEntry(t *testing.T, log *Log, message string, testID string, realtime uint64, monotonic uint64) {
	t.Helper()
	fields := []Field{StringField("MESSAGE", message)}
	if testID != "" {
		fields = append(fields, StringField("TEST_ID", testID))
	}
	if err := log.Append(fields, EntryOptions{RealtimeUsec: realtime, MonotonicUsec: monotonic}); err != nil {
		t.Fatalf("Append(%s) error = %v", message, err)
	}
}

func closeLogForTest(t *testing.T, log *Log, label string) {
	t.Helper()
	if err := log.Close(); err != nil {
		t.Fatalf("Close(%s) error = %v", label, err)
	}
}

func syncLogForTest(t *testing.T, log *Log, label string) {
	t.Helper()
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync(%s) error = %v", label, err)
	}
}

func forceCloseActiveWriter(t *testing.T, log *Log, label string) string {
	t.Helper()
	activePath := log.ActivePath()
	if err := log.writer.Close(); err != nil {
		t.Fatalf("writer.Close(%s active) error = %v", label, err)
	}
	log.writer = nil
	log.closed = true
	return activePath
}

func assertJournalFileCount(t *testing.T, dir string, context string, want int) []string {
	t.Helper()
	files := journalFiles(t, dir)
	if len(files) != want {
		t.Fatalf("%s journal file count = %d, want %d; files=%v", context, len(files), want, files)
	}
	return files
}

func assertOnlyJournalFile(t *testing.T, dir string, context string, wantPath string) {
	t.Helper()
	files := journalFiles(t, dir)
	if len(files) != 1 || files[0] != wantPath {
		t.Fatalf("%s journal files = %v, want only %s", context, files, wantPath)
	}
}

func assertJournalHeadSeqnums(t *testing.T, files []string, wantHeads []uint64, verify bool) {
	t.Helper()
	for i, path := range files {
		if verify {
			verifyJournalctl(t, path)
		}
		snapshot := readJournalSnapshot(t, path)
		if snapshot.header.headEntrySeqnum != wantHeads[i] {
			t.Fatalf("%s head seqnum = %d, want %d", path, snapshot.header.headEntrySeqnum, wantHeads[i])
		}
	}
}

func assertJournalChainHeads(t *testing.T, files []string, wantHeads []uint64) {
	t.Helper()
	var seqnumID UUID
	for i, path := range files {
		snapshot := readJournalSnapshot(t, path)
		if i == 0 {
			seqnumID = snapshot.header.seqnumID
		} else if snapshot.header.seqnumID != seqnumID {
			t.Fatalf("%s seqnum id = %s, want resumed chain id %s", path, snapshot.header.seqnumID, seqnumID)
		}
		if snapshot.header.headEntrySeqnum != wantHeads[i] {
			t.Fatalf("%s head seqnum = %d, want %d", path, snapshot.header.headEntrySeqnum, wantHeads[i])
		}
	}
}

func assertJournalSeqnumRange(t *testing.T, path string, context string, wantHead uint64, wantTail uint64) {
	t.Helper()
	snapshot := readJournalSnapshot(t, path)
	if snapshot.header.headEntrySeqnum != wantHead || snapshot.header.tailEntrySeqnum != wantTail {
		t.Fatalf("%s seqnum range = [%d,%d], want [%d,%d]", context, snapshot.header.headEntrySeqnum, snapshot.header.tailEntrySeqnum, wantHead, wantTail)
	}
}

func assertDirectoryJSONRows(t *testing.T, dir string, match string, context string, want int) []map[string]any {
	t.Helper()
	rows := runJournalctlDirectoryJSON(t, dir, match)
	if len(rows) != want {
		t.Fatalf("%s row count = %d, want %d", context, len(rows), want)
	}
	return rows
}

func assertDisposedJournalCount(t *testing.T, dir string, want int) {
	t.Helper()
	if disposed := disposedJournalFiles(t, dir); len(disposed) != want {
		t.Fatalf("disposed journal count = %d, want %d; files=%v", len(disposed), want, disposed)
	}
}

func assertPathDoesNotExist(t *testing.T, path string, context string) {
	t.Helper()
	if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("%s stat error = %v, want not exist", context, err)
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
	sort.Strings(files)
	return files
}

func disposedJournalFiles(t *testing.T, dir string) []string {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("ReadDir(%s) error = %v", dir, err)
	}
	var files []string
	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), ".journal~") {
			files = append(files, filepath.Join(dir, entry.Name()))
		}
	}
	sort.Strings(files)
	return files
}

func clearKeyedHashFlag(t *testing.T, path string) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatalf("OpenFile(%s) error = %v", path, err)
	}
	defer f.Close()
	buf := make([]byte, 4)
	if _, err := f.ReadAt(buf, 12); err != nil {
		t.Fatalf("ReadAt(incompatible flags) error = %v", err)
	}
	flags := binary.LittleEndian.Uint32(buf)
	binary.LittleEndian.PutUint32(buf, flags&^incompatibleKeyedHash)
	if _, err := f.WriteAt(buf, 12); err != nil {
		t.Fatalf("WriteAt(incompatible flags) error = %v", err)
	}
}

func writeHeaderSize(t *testing.T, path string, size uint64) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatalf("OpenFile(%s) error = %v", path, err)
	}
	defer f.Close()
	buf := make([]byte, 8)
	binary.LittleEndian.PutUint64(buf, size)
	if _, err := f.WriteAt(buf, 88); err != nil {
		t.Fatalf("WriteAt(header size) error = %v", err)
	}
}

func equalUint64s(a, b []uint64) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func requireJournalctl(t *testing.T) {
	t.Helper()
	if runtime.GOOS != "linux" {
		t.Skip("stock journalctl validation is Linux-only")
	}
	if !journalctlAvailable() {
		t.Skip("journalctl is not installed")
	}
}

func journalctlAvailable() bool {
	if runtime.GOOS != "linux" {
		return false
	}
	if _, err := exec.LookPath("journalctl"); err != nil {
		return false
	}
	return true
}

func runJournalctlDirectoryJSON(t *testing.T, dir string, matches ...string) []map[string]any {
	t.Helper()
	requireJournalctl(t)

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

func parseU64JSONField(t *testing.T, row map[string]any, key string) uint64 {
	t.Helper()

	raw, ok := row[key]
	if !ok {
		t.Fatalf("field %s missing from row %v", key, row)
	}
	value, err := strconv.ParseUint(fmt.Sprint(raw), 10, 64)
	if err != nil {
		t.Fatalf("field %s value %v parse error = %v", key, raw, err)
	}
	return value
}
