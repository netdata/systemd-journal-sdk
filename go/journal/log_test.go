package journal

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
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
	if files := journalFiles(t, dir); len(files) != 0 {
		t.Fatalf("journal files after empty append = %d, want 0; files=%v", len(files), files)
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
	if len(files) != 1 {
		t.Fatalf("journal file count before close = %d, want 1 current file; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, log.ActivePath())
	if snapshot.header.state != stateOnline {
		t.Fatalf("active state = %d, want online", snapshot.header.state)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-active-retention")
	if len(rows) != 1 {
		t.Fatalf("active-retention row count = %d, want 1", len(rows))
	}
	assertJSONField(t, rows[0], "MESSAGE", "active-retained-2")

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
	if len(files) != 1 {
		t.Fatalf("journal files after byte retention = %d, want 1 protected final file; files=%v", len(files), files)
	}
}

func TestLogEnforceRetentionDeletesFilesByAgeWithoutAppend(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}
	first, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 3; i++ {
		if err := first.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("age-retention-%d", i)),
		}, EntryOptions{RealtimeUsec: uint64(1_000_000 + i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	if files := journalFiles(t, dir); len(files) != 3 {
		t.Fatalf("initial journal file count = %d, want 3; files=%v", len(files), files)
	}

	retainedConfig := config
	retainedConfig.RotationPolicy = RotationPolicy{}
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxAge(time.Second)
	retained, err := NewLog(root, retainedConfig)
	if err != nil {
		t.Fatalf("NewLog(retained) error = %v", err)
	}
	if files := journalFiles(t, dir); len(files) != 3 {
		t.Fatalf("construction enforced age retention; files=%v", files)
	}
	if err := retained.EnforceRetention(); err != nil {
		t.Fatalf("EnforceRetention() error = %v", err)
	}
	if files := journalFiles(t, dir); len(files) != 0 {
		t.Fatalf("journal file count after age retention = %d, want 0; files=%v", len(files), files)
	}
	if err := retained.Close(); err != nil {
		t.Fatalf("Close(retained) error = %v", err)
	}
}

func TestLogEnforceRetentionProtectsActiveFileByAge(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}
	first, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 2; i++ {
		if err := first.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("age-active-retention-%d", i)),
		}, EntryOptions{RealtimeUsec: uint64(1_000_000 + i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	if files := journalFiles(t, dir); len(files) != 2 {
		t.Fatalf("initial journal file count = %d, want 2; files=%v", len(files), files)
	}

	retainedConfig := config
	retainedConfig.RotationPolicy = RotationPolicy{}
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxAge(time.Second)
	retained, err := NewLog(root, retainedConfig)
	if err != nil {
		t.Fatalf("NewLog(retained) error = %v", err)
	}
	if err := retained.Append([]Field{
		StringField("MESSAGE", "age-protected-active"),
	}, EntryOptions{RealtimeUsec: 1_000_100, MonotonicUsec: 10}); err != nil {
		t.Fatalf("Append(active) error = %v", err)
	}
	activePath := retained.ActivePath()
	if err := retained.EnforceRetention(); err != nil {
		t.Fatalf("EnforceRetention() error = %v", err)
	}
	files := journalFiles(t, dir)
	if len(files) != 1 || files[0] != activePath {
		t.Fatalf("journal files after active age retention = %v, want only active %s", files, activePath)
	}
	snapshot := readJournalSnapshot(t, activePath)
	if snapshot.header.state != stateOnline {
		t.Fatalf("active state = %d, want online", snapshot.header.state)
	}
	if err := retained.Close(); err != nil {
		t.Fatalf("Close(retained) error = %v", err)
	}
}

func TestLogStrictCloseProtectsCurrentArchiveFromByteRetention(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options:             testOptions(),
		Source:              "system",
		StrictSystemdNaming: true,
		RetentionPolicy:     RetentionPolicy{}.WithMaxBytes(1),
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "strict-byte-retained"),
		StringField("TEST_ID", "directory-strict-byte-retention"),
	}, EntryOptions{RealtimeUsec: 1_700_002_230_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("Append(strict byte retention) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(strict byte retention) error = %v", err)
	}

	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("journal files after strict byte retention = %d, want 1 protected current archive; files=%v", len(files), files)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-strict-byte-retention")
	if len(rows) != 1 {
		t.Fatalf("strict byte retention row count = %d, want 1", len(rows))
	}
	assertJSONField(t, rows[0], "MESSAGE", "strict-byte-retained")
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

func TestLogReopensActiveFileAndContinuesSequence(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:             testOptions(),
		Source:              "system",
		RotationPolicy:      RotationPolicy{}.WithMaxEntries(3),
		StrictSystemdNaming: true,
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

func TestLogDefaultChainReopenContinuesSequence(t *testing.T) {
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
			StringField("MESSAGE", fmt.Sprintf("chain-reopen-%d", i)),
			StringField("TEST_ID", "directory-chain-reopen"),
		}, EntryOptions{RealtimeUsec: 1_700_002_260_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	second, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(second) error = %v", err)
	}
	if err := second.Append([]Field{
		StringField("MESSAGE", "chain-reopen-2"),
		StringField("TEST_ID", "directory-chain-reopen"),
	}, EntryOptions{RealtimeUsec: 1_700_002_260_000_002, MonotonicUsec: 3}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := journalFiles(t, dir)
	if len(files) != 2 {
		t.Fatalf("journal file count after chain reopen = %d, want 2; files=%v", len(files), files)
	}
	wantHeads := []uint64{1, 3}
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
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-chain-reopen")
	if len(rows) != 3 {
		t.Fatalf("chain reopen row count = %d, want 3", len(rows))
	}
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

func TestLogDiscardsEmptyOnlineFileAndContinuesSequence(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options: testOptions(),
		Source:  "system",
	}
	first, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 2; i++ {
		if err := first.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("empty-reopen-%d", i)),
			StringField("TEST_ID", "directory-empty-online-reopen"),
		}, EntryOptions{RealtimeUsec: 1_700_002_272_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	files := journalFiles(t, dir)
	if len(files) != 1 {
		t.Fatalf("file count after first close = %d, want 1; files=%v", len(files), files)
	}
	snapshot := readJournalSnapshot(t, files[0])
	nextSeqnum := snapshot.header.tailEntrySeqnum + 1
	emptyPath := filepath.Join(
		dir,
		fmt.Sprintf("system@%s-%016x-%016x.journal", snapshot.header.seqnumID.String(), nextSeqnum, 1_700_002_272_000_010),
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
	if err := empty.Close(); err != nil {
		t.Fatalf("Close(empty active) error = %v", err)
	}

	second, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(second) error = %v", err)
	}
	if err := second.Append([]Field{
		StringField("MESSAGE", "empty-reopen-2"),
		StringField("TEST_ID", "directory-empty-online-reopen"),
	}, EntryOptions{RealtimeUsec: 1_700_002_272_000_002, MonotonicUsec: 3}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}
	if _, err := os.Stat(emptyPath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("empty active stat error = %v, want not exist", err)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=directory-empty-online-reopen")
	if len(rows) != 3 {
		t.Fatalf("empty-online row count = %d, want 3", len(rows))
	}
	assertJSONField(t, rows[2], "MESSAGE", "empty-reopen-2")
}

func TestNewLogDoesNotEnforceRetentionBeforeFirstAppend(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}

	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 2; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("construction-retention-%d", i)),
			StringField("TEST_ID", "newlog-no-construction-retention"),
		}, EntryOptions{RealtimeUsec: 1_700_002_275_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(first %d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	before := journalFiles(t, dir)
	if len(before) != 2 {
		t.Fatalf("archive count before second NewLog = %d, want 2; files=%v", len(before), before)
	}

	retainedConfig := config
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxFiles(1)
	reopened, err := NewLog(root, retainedConfig)
	if err != nil {
		t.Fatalf("NewLog(second) error = %v", err)
	}
	after := journalFiles(t, dir)
	if len(after) != 2 {
		t.Fatalf("archive count after second NewLog = %d, want 2; files=%v", len(after), after)
	}
	if err := reopened.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}
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

func TestNewLogStrictIdentityRequiresMachineAndBootID(t *testing.T) {
	_, err := NewLog(t.TempDir(), LogConfig{IdentityMode: LogIdentityStrict})
	if !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(strict identity without IDs) error = %v, want ErrInvalidJournal", err)
	}

	options := testOptions()
	log, err := NewLog(t.TempDir(), LogConfig{
		Options:      options,
		IdentityMode: LogIdentityStrict,
	})
	if err != nil {
		t.Fatalf("NewLog(strict identity with IDs) error = %v", err)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestNewLogRejectsExplicitZeroPolicyLimits(t *testing.T) {
	if _, err := NewLog(t.TempDir(), LogConfig{
		Options:        testOptions(),
		RotationPolicy: RotationPolicy{}.WithMaxEntries(0),
	}); !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(zero max entries) error = %v, want ErrInvalidJournal", err)
	}
	if _, err := NewLog(t.TempDir(), LogConfig{
		Options:         testOptions(),
		RetentionPolicy: RetentionPolicy{}.WithMaxBytes(0),
	}); !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("NewLog(zero max bytes) error = %v, want ErrInvalidJournal", err)
	}
}

func TestLogAppendAddsSourceRealtimeAndClampsEntryRealtime(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.Append([]Field{
		StringField("MESSAGE", "source realtime one"),
		StringField("TEST_ID", "source-realtime-clamp"),
	}, EntryOptions{
		RealtimeUsec:       1_700_002_800_000_000,
		MonotonicUsec:      10,
		SourceRealtimeUsec: 1_600_000_000_000_000,
	}); err != nil {
		t.Fatalf("Append(first) error = %v", err)
	}
	if err := log.Append([]Field{
		StringField("MESSAGE", "source realtime two"),
		StringField("TEST_ID", "source-realtime-clamp"),
	}, EntryOptions{
		RealtimeUsec:       1_700_002_799_999_000,
		MonotonicUsec:      5,
		SourceRealtimeUsec: 1_600_000_000_000_001,
	}); err != nil {
		t.Fatalf("Append(second) error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}

	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=source-realtime-clamp")
	if len(rows) != 2 {
		t.Fatalf("row count = %d, want 2", len(rows))
	}
	assertJSONField(t, rows[0], "_SOURCE_REALTIME_TIMESTAMP", "1600000000000000")
	assertJSONField(t, rows[1], "_SOURCE_REALTIME_TIMESTAMP", "1600000000000001")
	if got := parseU64JSONField(t, rows[1], "__REALTIME_TIMESTAMP"); got != 1_700_002_800_000_001 {
		t.Fatalf("second realtime = %d, want clamped 1700002800000001", got)
	}
	if got := parseU64JSONField(t, rows[1], "__MONOTONIC_TIMESTAMP"); got != 11 {
		t.Fatalf("second monotonic = %d, want clamped 11", got)
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogAppendMapWithOptionsAddsSourceRealtime(t *testing.T) {
	requireJournalctl(t)

	log, dir := newTestLog(t, LogConfig{
		Options: testOptions(),
		Source:  "system",
	})
	if err := log.AppendMapWithOptions(map[string]string{
		"MESSAGE": "map source realtime",
		"TEST_ID": "map-source-realtime",
	}, EntryOptions{
		RealtimeUsec:       1_700_002_850_000_000,
		MonotonicUsec:      1,
		SourceRealtimeUsec: 1_600_000_100_000_000,
	}); err != nil {
		t.Fatalf("AppendMapWithOptions() error = %v", err)
	}
	if err := log.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	rows := runJournalctlDirectoryJSON(t, dir, "TEST_ID=map-source-realtime")
	if len(rows) != 1 {
		t.Fatalf("row count = %d, want 1", len(rows))
	}
	assertJSONField(t, rows[0], "_SOURCE_REALTIME_TIMESTAMP", "1600000100000000")
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestLogLifecycleReportsRotationAndRetentionDelete(t *testing.T) {
	root := t.TempDir()
	var events []LogLifecycleEvent
	log, err := NewLog(root, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(2),
		Lifecycle:       LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) }),
	})
	if err != nil {
		t.Fatalf("NewLog() error = %v", err)
	}
	for i := 0; i < 3; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("lifecycle-%d", i)),
			StringField("TEST_ID", "lifecycle-events"),
		}, EntryOptions{RealtimeUsec: 1_700_002_900_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

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

func TestLogRetentionUsesArtifactSizer(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}
	log, err := NewLog(root, config)
	if err != nil {
		t.Fatalf("NewLog(first) error = %v", err)
	}
	for i := 0; i < 3; i++ {
		if err := log.Append([]Field{
			StringField("MESSAGE", fmt.Sprintf("artifact-size-%d", i)),
			StringField("TEST_ID", "artifact-size-retention"),
		}, EntryOptions{RealtimeUsec: 1_700_003_000_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := log.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	dir := filepath.Join(root, config.Options.MachineID.String())
	before := journalFiles(t, dir)
	if len(before) != 3 {
		t.Fatalf("archive count before artifact retention = %d, want 3; files=%v", len(before), before)
	}

	retainedConfig := config
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(^uint64(0) - 1)
	retainedConfig.ArtifactSizer = LogArtifactSizeFunc(func(string) (uint64, error) {
		return ^uint64(0) / 2, nil
	})
	retained, err := NewLog(root, retainedConfig)
	if err != nil {
		t.Fatalf("NewLog(retained) error = %v", err)
	}
	if err := retained.EnforceRetention(); err != nil {
		t.Fatalf("EnforceRetention() error = %v", err)
	}
	after := journalFiles(t, dir)
	if len(after) >= len(before) {
		t.Fatalf("archive count after artifact retention = %d, want less than %d; files=%v", len(after), len(before), after)
	}
	if err := retained.Close(); err != nil {
		t.Fatalf("Close(retained) error = %v", err)
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
