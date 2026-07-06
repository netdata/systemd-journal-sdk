package journal

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
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

func TestLogAppendRejectsEmptyEntryWithoutCreatingFile(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})

	if err := log.Append(nil, EntryOptions{}); !errors.Is(err, errEntryEmpty) {
		t.Fatalf("Append(nil) error = %v, want errEntryEmpty", err)
	}
	if err := log.AppendRaw(nil, EntryOptions{}); !errors.Is(err, errEntryEmpty) {
		t.Fatalf("AppendRaw(nil) error = %v, want errEntryEmpty", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("NO_EQUALS")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(no equals) error = %v, want errFieldName", err)
	}
	if err := log.AppendRaw([][]byte{nil}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(empty payload) error = %v, want errFieldName", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("=")}, EntryOptions{}); !errors.Is(err, errFieldName) {
		t.Fatalf("AppendRaw(single equals) error = %v, want errFieldName", err)
	}
	if files := journalFiles(t, dir); len(files) != 0 {
		t.Fatalf("journal files after empty append = %d, want 0; files=%v", len(files), files)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogAppendRejectsMissingMonotonicWithoutCreatingFile(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})

	if err := log.Append([]Field{StringField("MESSAGE", "missing monotonic")}, EntryOptions{}); !errors.Is(err, ErrMissingMonotonicUsec) {
		t.Fatalf("Append(missing monotonic) error = %v, want ErrMissingMonotonicUsec", err)
	}
	if err := log.AppendRaw([][]byte{[]byte("MESSAGE=missing monotonic")}, EntryOptions{}); !errors.Is(err, ErrMissingMonotonicUsec) {
		t.Fatalf("AppendRaw(missing monotonic) error = %v, want ErrMissingMonotonicUsec", err)
	}
	if files := journalFiles(t, dir); len(files) != 0 {
		t.Fatalf("journal files after missing monotonic append = %d, want 0; files=%v", len(files), files)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestAlign8SaturatingDoesNotWrap(t *testing.T) {
	if got, want := align8Saturating(^uint64(0)), ^uint64(0)&^uint64(7); got != want {
		t.Fatalf("align8Saturating(max) = %d, want %d", got, want)
	}
}

func TestLogCloseRemovesEmptyActiveFile(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
	})
	if err := log.ensureWriter(EntryOptions{}, LogLifecycleReasonEagerOpen); err != nil {
		t.Fatalf("ensureWriter() error = %v", err)
	}
	activePath := log.ActivePath()
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

func TestLogDefaultUsesNetdataChainActiveNaming(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "default chain naming"),
		StringField("TEST_ID", "directory-default-chain-naming"),
	}, EntryOptions{RealtimeUsec: 1_700_002_060_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(default chain) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}

	activeBase := filepath.Base(log.ActivePath())
	if !strings.HasPrefix(activeBase, "system@") || !strings.HasSuffix(activeBase, ".journal") {
		t.Fatalf("active filename = %q, want Netdata chain naming", activeBase)
	}
	if _, err := os.Stat(filepath.Join(dir, "system.journal")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("system.journal stat error = %v, want not exist in default naming mode", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-default-chain-naming")
	if len(rows) != 1 {
		t.Fatalf("default chain row count = %d, want 1", len(rows))
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogStrictSystemdNamingUsesSourceJournalActive(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "strict active naming"),
		StringField("TEST_ID", "directory-strict-systemd-naming"),
	}, EntryOptions{RealtimeUsec: 1_700_002_065_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(strict naming) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	if base := filepath.Base(log.ActivePath()); base != "system.journal" {
		t.Fatalf("active filename = %q, want system.journal", base)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-strict-systemd-naming")
	if len(rows) != 1 {
		t.Fatalf("strict naming row count = %d, want 1", len(rows))
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal file count after strict close = %d, want 1; files=%v", len(files), files)
	}
	if base := filepath.Base(files[0]); !strings.HasPrefix(base, "system@") {
		t.Fatalf("archived filename = %q, want system@*.journal", base)
	}
}

func TestLogCustomSourceNaming(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "custom-source",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "custom default source"),
	}, EntryOptions{RealtimeUsec: 1_700_002_066_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(custom default) error = %v", err)
	}
	if base := filepath.Base(log.ActivePath()); !strings.HasPrefix(base, "custom-source@") {
		t.Fatalf("active filename = %q, want custom-source@*.journal", base)
	}
	if _, err := os.Stat(filepath.Join(dir, "custom-source.journal")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("custom-source.journal stat error = %v, want not exist in default mode", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(custom default) error = %v", err)
	}

	strict, strictDir := newTestLog(t, LogConfig{
		Options:             testOptions(),
		Source:              "custom-source",
		StrictSystemdNaming: true,
	})
	if err := strict.Append([]Field{
		StringField("MESSAGE", "custom strict source"),
	}, EntryOptions{RealtimeUsec: 1_700_002_066_000_001, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(custom strict) error = %v", err)
	}
	if base := filepath.Base(strict.ActivePath()); base != "custom-source.journal" {
		t.Fatalf("strict active filename = %q, want custom-source.journal", base)
	}
	if err := strict.Close(); err != nil {
		t.Fatalf("Close(custom strict) error = %v", err)
	}
	files := journalFiles(t, strictDir)
	if len(files) != 1 || !strings.HasPrefix(filepath.Base(files[0]), "custom-source@") {
		t.Fatalf("strict archived files = %v, want custom-source@*.journal", files)
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

func TestLogRotatesByDuration(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxDuration(10 * time.Second),
	})

	base := uint64(1_700_002_090_000_000)
	for i, realtime := range []uint64{base, base + 9_999_999, base + 10_000_000} {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("duration-rotation-%d", i)),
			StringField("TEST_ID", "directory-duration-rotation"),
		}, EntryOptions{RealtimeUsec: realtime, MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal file count after duration rotation = %d, want 2; files=%v", len(files), files)
	}
	counts := make([]uint64, 0, len(files))
	for _, file := range files {
		counts = append(counts, readJournalSnapshot(t, file).header.nEntries)
	}
	if got, want := counts, []uint64{2, 1}; !equalUint64s(got, want) {
		t.Fatalf("duration rotation entry counts = %v, want %v", got, want)
	}
}

func TestLogStrictReopenContinuesSequenceAfterClose(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
	}
	first, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first strict) error = %v", err)
	}
	if err := first.Append([]Field{
		StringField("MESSAGE", "strict-reopen-0"),
		StringField("TEST_ID", "directory-strict-reopen"),
	}, EntryOptions{RealtimeUsec: 1_700_002_240_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(first strict) error = %v", err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first strict) error = %v", err)
	}
	if got := first.ActivePath(); got != "" {
		t.Fatalf("ActivePath after strict close = %q, want empty", got)
	}

	second, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(second strict) error = %v", err)
	}
	if err := second.Append([]Field{
		StringField("MESSAGE", "strict-reopen-1"),
		StringField("TEST_ID", "directory-strict-reopen"),
	}, EntryOptions{RealtimeUsec: 1_700_002_240_000_001, MonotonicUsec: 2}); err != nil {
		t.Fatalf("Append(second strict) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second strict) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-strict-reopen")
	if len(rows) != 2 {
		t.Fatalf("strict reopen row count = %d, want 2", len(rows))
	}
	if got := rows[0]["__SEQNUM"].(string); got != "1" {
		t.Fatalf("first strict seqnum = %s, want 1", got)
	}
	if got := rows[1]["__SEQNUM"].(string); got != "2" {
		t.Fatalf("second strict seqnum = %s, want 2", got)
	}
}

func TestLogStrictEmptyCloseClearsActivePath(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
		OpenMode:            LogOpenEager,
	}
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(strict eager) error = %v", err)
	}
	activePath := log.ActivePath()
	if activePath == "" {
		t.Fatalf("ActivePath before empty close is empty")
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(empty strict) error = %v", err)
	}
	if got := log.ActivePath(); got != "" {
		t.Fatalf("ActivePath after empty strict close = %q, want empty", got)
	}
	if _, err := os.Stat(activePath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("empty strict active stat error = %v, want not exist", err)
	}
}

func TestLogReopensActiveFileAndContinuesSequence(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:             testOptions(),
		Source:              "system",
		RotationPolicy:      RotationPolicy{}.WithMaxEntries(3),
		StrictSystemdNaming: true,
	}
	log := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, log, "reopen", "directory-reopen", 0, 2, 1_700_002_250_000_000)
	syncLogForTest(t, log, "first")
	activePath := forceCloseActiveWriter(t, log, "first")

	reopened := mustNewLogForTest(t, root, config, "reopen")
	if reopened.ActivePath() != activePath {
		t.Fatalf("ActivePath after reopen = %q, want %q", reopened.ActivePath(), activePath)
	}
	appendLogRange(t, reopened, "reopen", "directory-reopen", 2, 4, 1_700_002_250_000_000)
	closeLogForTest(t, reopened, "reopen")

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := assertJournalFileCount(t, dir, "after reopen", 2)
	assertJournalHeadSeqnums(t, files, []uint64{1, 4}, true)
	rows := assertDirectoryJSONRows(t, dir, "TEST_ID=directory-reopen", "reopen", 4)
	for i := 0; i < 4; i++ {
		assertJSONField(t, rows[i], "MESSAGE", fmt.Sprintf("reopen-%d", i))
	}
}

func TestLogDefaultChainReopenContinuesSequence(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(10),
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "chain-reopen", "directory-chain-reopen", 0, 2, 1_700_002_260_000_000)
	closeLogForTest(t, first, "first")

	second := mustNewLogForTest(t, root, config, "second")
	appendLogEntry(t, second, "chain-reopen-2", "directory-chain-reopen", 1_700_002_260_000_002, 3)
	closeLogForTest(t, second, "second")

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := assertJournalFileCount(t, dir, "after chain reopen", 2)
	assertJournalChainHeads(t, files, []uint64{1, 3})
	assertDirectoryJSONRows(t, dir, "TEST_ID=directory-chain-reopen", "chain reopen", 3)
}

func TestLogDefaultChainReopensOnlineFile(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(10),
	}
	first, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 2; i++ {
		if err := first.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("chain-online-reopen-%d", i)),
			StringField("TEST_ID", "directory-chain-online-reopen"),
		}, EntryOptions{RealtimeUsec: 1_700_002_270_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := first.Sync(); err != nil {
		t.Fatalf("Sync(first) error = %v", err)
	}
	activePath := first.ActivePath()
	if err := first.writer.Close(); err != nil {
		t.Fatalf("writer.Close(first active) error = %v", err)
	}
	first.writer = nil
	first.closed = true

	second, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(second) error = %v", err)
	}
	if second.ActivePath() != activePath {
		t.Fatalf("ActivePath after default reopen = %q, want %q", second.ActivePath(), activePath)
	}
	if err := second.Append([]Field{
		StringField("MESSAGE", "chain-online-reopen-2"),
		StringField("TEST_ID", "directory-chain-online-reopen"),
	}, EntryOptions{RealtimeUsec: 1_700_002_270_000_002, MonotonicUsec: 3}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}
	snapshot := readJournalSnapshot(t, activePath)
	if snapshot.header.tailEntrySeqnum != 3 {
		t.Fatalf("tail seqnum after online reopen = %d, want 3", snapshot.header.tailEntrySeqnum)
	}
}

func TestLogStrictSystemdNamingArchivesOnlineChainActive(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(10),
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "strict-migrate", "directory-strict-migrate-online-chain", 0, 2, 1_700_002_271_000_000)
	syncLogForTest(t, first, "first")
	chainPath := forceCloseActiveWriter(t, first, "first")

	strictConfig := config
	strictConfig.StrictSystemdNaming = true
	strict := mustNewLogForTest(t, root, strictConfig, "strict")
	if snapshot := readJournalSnapshot(t, chainPath); snapshot.header.state != stateArchived {
		t.Fatalf("chain active state after strict open = %d, want archived", snapshot.header.state)
	}
	appendLogEntry(t, strict, "strict-migrate-2", "directory-strict-migrate-online-chain", 1_700_002_271_000_002, 3)
	if base := filepath.Base(strict.ActivePath()); base != "system.journal" {
		t.Fatalf("strict active filename = %q, want system.journal", base)
	}
	assertJournalSeqnumRange(t, strict.ActivePath(), "strict active", 3, 3)
	dir := filepath.Join(root, config.Options.MachineID.String())
	assertDirectoryJSONRows(t, dir, "TEST_ID=directory-strict-migrate-online-chain", "strict migration", 3)
	closeLogForTest(t, strict, "strict")
}

func TestLogReplacesUnsupportedDefaultChainActive(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(10),
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "replace-chain", "", 0, 2, 1_700_002_272_000_000)
	syncLogForTest(t, first, "first")
	activePath := forceCloseActiveWriter(t, first, "first")

	clearKeyedHashFlag(t, activePath)
	if _, err := Open(activePath); !errors.Is(err, errUnsupportedJournal) {
		t.Fatalf("Open(unkeyed active) error = %v, want errUnsupportedJournal", err)
	}

	second := mustNewLogForTest(t, root, config, "second")
	assertPathDoesNotExist(t, activePath, "old active")
	assertDisposedJournalCount(t, second.JournalDirectory(), 1)
	appendLogEntry(t, second, "replace-chain-2", "", 1_700_002_272_000_002, 3)
	if second.ActivePath() == activePath {
		t.Fatalf("new active path reused unsupported active path %q", activePath)
	}
	assertJournalSeqnumRange(t, second.ActivePath(), "replacement", 3, 3)
	closeLogForTest(t, second, "second")
}

func TestLogReplacesOutdatedStrictActive(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
		RotationPolicy:      RotationPolicy{}.WithMaxEntries(10),
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "replace-strict", "", 0, 2, 1_700_002_273_000_000)
	syncLogForTest(t, first, "first")
	activePath := forceCloseActiveWriter(t, first, "first")

	writeHeaderSize(t, activePath, headerSize-8)
	if _, err := Open(activePath); !errors.Is(err, errUnsupportedJournal) {
		t.Fatalf("Open(outdated active) error = %v, want errUnsupportedJournal", err)
	}

	second := mustNewLogForTest(t, root, config, "second")
	assertPathDoesNotExist(t, activePath, "old strict active")
	assertDisposedJournalCount(t, second.JournalDirectory(), 1)
	appendLogEntry(t, second, "replace-strict-2", "", 1_700_002_273_000_002, 3)
	if filepath.Base(second.ActivePath()) != "system.journal" {
		t.Fatalf("strict active filename = %q, want system.journal", filepath.Base(second.ActivePath()))
	}
	assertJournalSeqnumRange(t, second.ActivePath(), "replacement", 3, 3)
	closeLogForTest(t, second, "second")
}

func TestLogDiscardsEmptyOnlineFileAndContinuesSequence(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options: testOptions(),
		Source:  "system",
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "empty-reopen", "directory-empty-online-reopen", 0, 2, 1_700_002_272_000_000)
	closeLogForTest(t, first, "first")

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := assertJournalFileCount(t, dir, "after first close", 1)
	emptyPath := createEmptyOnlineContinuation(t, dir, files[0], config)

	second := mustNewLogForTest(t, root, config, "second")
	appendLogEntry(t, second, "empty-reopen-2", "directory-empty-online-reopen", 1_700_002_272_000_002, 3)
	closeLogForTest(t, second, "second")
	assertPathDoesNotExist(t, emptyPath, "empty active")
	rows := assertDirectoryJSONRows(t, dir, "TEST_ID=directory-empty-online-reopen", "empty-online", 3)
	assertJSONField(t, rows[2], "MESSAGE", "empty-reopen-2")
}

func createEmptyOnlineContinuation(t *testing.T, dir string, previousPath string, config LogConfig) string {
	t.Helper()
	snapshot := readJournalSnapshot(t, previousPath)
	nextSeqnum := snapshot.header.tailEntrySeqnum + 1
	emptyPath := filepath.Join(
		dir,
		fmt.Sprintf("system@%s-%016x-%016x.journal", snapshot.header.seqnumID.String(), nextSeqnum, uint64(1_700_002_272_000_010)),
	)
	empty, err := Create(emptyPath, Options{
		MachineID:  config.Options.MachineID,
		BootID:     config.Options.BootID,
		SeqnumID:   snapshot.header.seqnumID,
		HeadSeqnum: nextSeqnum,
	})
	if err != nil {
		t.Fatalf("Create(empty active) error = %v", err)
	}
	closeWriterForTest(t, empty, "empty active")
	return emptyPath
}

func TestLogAPIExposesConfiguredAndJournalDirectories(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options: testOptions(),
		Source:  "system",
	}
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog() error = %v", err)
	}
	if got := log.ConfiguredDirectory(); got != root {
		t.Fatalf("ConfiguredDirectory() = %q, want %q", got, root)
	}
	wantDir := filepath.Join(root, config.Options.MachineID.String())
	if got := log.JournalDirectory(); got != wantDir {
		t.Fatalf("JournalDirectory() = %q, want %q", got, wantDir)
	}
	if got := log.MachineID(); got != config.Options.MachineID {
		t.Fatalf("MachineID() = %s, want %s", got.String(), config.Options.MachineID.String())
	}
	if got := log.BootID(); got != config.Options.BootID {
		t.Fatalf("BootID() = %s, want %s", got.String(), config.Options.BootID.String())
	}
	if got := log.Source(); got != "system" {
		t.Fatalf("Source() = %q, want system", got)
	}
	if got := log.ActivePath(); got != "" {
		t.Fatalf("ActivePath() before lazy append = %q, want empty", got)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestNewLogEagerOpenCreatesActiveAndReportsLifecycle(t *testing.T) {
	root := t.TempDir()
	var events []LogLifecycleEvent
	log, err := NewLog(root, LogConfig{
		Options:   testOptions(),
		Source:    "system",
		OpenMode:  LogOpenEager,
		Lifecycle: LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) }),
	})
	if err != nil {
		t.Fatalf("NewLog() error = %v", err)
	}
	activePath := log.ActivePath()
	if activePath == "" {
		t.Fatalf("ActivePath() after eager open is empty")
	}
	if _, err := os.Stat(activePath); err != nil {
		t.Fatalf("active stat error = %v", err)
	}
	if len(events) != 1 {
		t.Fatalf("event count after eager open = %d, want 1: %#v", len(events), events)
	}
	if events[0].Type != LogLifecycleCreated || events[0].Reason != LogLifecycleReasonEagerOpen || events[0].ActivePath != activePath {
		t.Fatalf("eager event = %#v, want created/eager_open for %s", events[0], activePath)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogLifecycleReportsRotationAndRetentionDelete(t *testing.T) {
	root := t.TempDir()
	var events []LogLifecycleEvent
	log := mustNewLogForTest(t, root, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(2),
		Lifecycle:       LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) }),
	}, "lifecycle")
	appendLogRange(t, log, "lifecycle", "lifecycle-events", 0, 3, 1_700_002_900_000_000)
	closeLogForTest(t, log, "lifecycle")
	assertLifecycleHasCreateRotateDelete(t, events)
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

func TestLogSyncOnArchiveDefaultSyncsOnCallerPath(t *testing.T) {
	log, _ := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	})

	oldSync := syncArchiveJournalFile
	syncCalls := 0
	syncArchiveJournalFile = func(w *Writer) error {
		syncCalls++
		return oldSync(w)
	}
	defer func() {
		syncArchiveJournalFile = oldSync
	}()

	if err := log.Append([]Field{
		StringField("MESSAGE", "default archive sync 0"),
	}, EntryOptions{RealtimeUsec: 1_700_002_650_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "default archive sync 1"),
	}, EntryOptions{RealtimeUsec: 1_700_002_650_000_001, MonotonicUsec: 2}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if syncCalls != 1 {
		t.Fatalf("archive sync calls after rotation = %d, want 1", syncCalls)
	}
}

func TestLogSyncOnArchiveFalseSkipsCallerPathSyncAndKeepsFilesReadable(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
		SyncOnArchive:  SyncOnArchive(false),
	})

	oldSync := syncArchiveJournalFile
	syncCalls := 0
	syncArchiveJournalFile = func(*Writer) error {
		syncCalls++
		return errors.New("archive sync should not be called")
	}
	defer func() {
		syncArchiveJournalFile = oldSync
	}()

	if err := log.Append([]Field{
		StringField("MESSAGE", "opt-out archive sync 0"),
	}, EntryOptions{RealtimeUsec: 1_700_002_660_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "opt-out archive sync 1"),
	}, EntryOptions{RealtimeUsec: 1_700_002_660_000_001, MonotonicUsec: 2}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(opt-out) error = %v", err)
	}
	if syncCalls != 0 {
		t.Fatalf("archive sync calls = %d, want 0", syncCalls)
	}

	files := assertJournalFileCount(t, dir, "after opt-out close", 2)
	for _, path := range files {
		snapshot := readJournalSnapshot(t, path)
		if snapshot.header.nEntries != 1 {
			t.Fatalf("%s entries = %d, want 1", path, snapshot.header.nEntries)
		}
		if snapshot.header.state != stateArchived {
			t.Fatalf("%s state = %d, want archived", path, snapshot.header.state)
		}
	}
}

func TestLogSyncOnArchivePolicyAppliesToStrictStartupArchive(t *testing.T) {
	baseConfig := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(10),
	}
	for _, tc := range []struct {
		name     string
		optOut   bool
		wantSync int
	}{
		{name: "default", wantSync: 1},
		{name: "opt-out", optOut: true, wantSync: 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := t.TempDir()
			first := mustNewLogForTest(t, root, baseConfig, "first")
			appendLogRange(t, first, "startup-sync", "strict-startup-sync", 0, 2, 1_700_002_665_000_000)
			syncLogForTest(t, first, "first")
			chainPath := forceCloseActiveWriter(t, first, "first")

			strictConfig := baseConfig
			strictConfig.StrictSystemdNaming = true
			if tc.optOut {
				strictConfig.SyncOnArchive = SyncOnArchive(false)
			}

			oldSync := syncArchiveJournalFile
			syncCalls := 0
			syncArchiveJournalFile = func(w *Writer) error {
				syncCalls++
				return oldSync(w)
			}
			strict, err := NewLog(root, strictConfig)
			syncArchiveJournalFile = oldSync
			if err != nil {
				t.Fatalf("NewLog(strict) error = %v", err)
			}
			defer strict.Close()

			if syncCalls != tc.wantSync {
				t.Fatalf("strict startup archive sync calls = %d, want %d", syncCalls, tc.wantSync)
			}
			if snapshot := readJournalSnapshot(t, chainPath); snapshot.header.state != stateArchived {
				t.Fatalf("chain active state after strict open = %d, want archived", snapshot.header.state)
			}
		})
	}
}

func TestLogRotationRetriesAfterArchiveCleanupFailure(t *testing.T) {
	log, dir := newTestLog(t, LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "rotation cleanup failure 0"),
	}, EntryOptions{RealtimeUsec: 1_700_002_700_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}

	syntheticErr := errors.New("synthetic rotation archive sync failure")
	oldSync := syncJournalDirectory
	syncCalls := 0
	syncJournalDirectory = func(string) error {
		syncCalls++
		if syncCalls == 1 {
			return syntheticErr
		}
		return nil
	}
	err := log.Append([]Field{
		StringField("MESSAGE", "rotation cleanup failure 1"),
	}, EntryOptions{RealtimeUsec: 1_700_002_700_000_001, MonotonicUsec: 2})
	syncJournalDirectory = oldSync
	if !errors.Is(err, syntheticErr) {
		t.Fatalf("Append(rotation failure) error = %v, want %v", err, syntheticErr)
	}

	if err := log.Append([]Field{
		StringField("MESSAGE", "rotation cleanup retry"),
	}, EntryOptions{RealtimeUsec: 1_700_002_700_000_002, MonotonicUsec: 3}); err != nil {
		t.Fatalf("Append(retry) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(after retry) error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal files after retry = %d, want 2; files=%v", len(files), files)
	}
	first := readJournalSnapshot(t, files[0]).header
	second := readJournalSnapshot(t, files[1]).header
	if first.headEntrySeqnum != 1 || first.tailEntrySeqnum != 1 {
		t.Fatalf("first file seqnum range = [%d,%d], want [1,1]", first.headEntrySeqnum, first.tailEntrySeqnum)
	}
	if second.headEntrySeqnum != 2 || second.tailEntrySeqnum != 2 {
		t.Fatalf("second file seqnum range = [%d,%d], want [2,2]", second.headEntrySeqnum, second.tailEntrySeqnum)
	}
}
