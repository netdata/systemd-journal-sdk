package journal

import (
	"fmt"
	"path/filepath"
	"testing"
	"time"
)

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
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "age-active-retention", "", 0, 2, 1_000_000)
	closeLogForTest(t, first, "first")

	dir := filepath.Join(root, config.Options.MachineID.String())
	assertJournalFileCount(t, dir, "initial", 2)

	retainedConfig := config
	retainedConfig.RotationPolicy = RotationPolicy{}
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxAge(time.Second)
	retained := mustNewLogForTest(t, root, retainedConfig, "retained")
	appendLogEntry(t, retained, "age-protected-active", "", 1_000_100, 10)
	activePath := retained.ActivePath()
	if err := retained.EnforceRetention(); err != nil {
		t.Fatalf("EnforceRetention() error = %v", err)
	}
	assertOnlyJournalFile(t, dir, "after active age retention", activePath)
	snapshot := readJournalSnapshot(t, activePath)
	if snapshot.header.state != stateOnline {
		t.Fatalf("active state = %d, want online", snapshot.header.state)
	}
	closeLogForTest(t, retained, "retained")
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

func TestNewLogLazyRetentionRunsOnFirstOpen(t *testing.T) {
	requireJournalctl(t)

	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}

	log := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, log, "construction-retention", "newlog-no-construction-retention", 0, 2, 1_700_002_275_000_000)
	closeLogForTest(t, log, "first")

	dir := filepath.Join(root, config.Options.MachineID.String())
	assertJournalFileCount(t, dir, "archive before second NewLog", 2)

	retainedConfig := config
	retainedConfig.RetentionPolicy = RetentionPolicy{}.WithMaxFiles(1)
	var events []LogLifecycleEvent
	retainedConfig.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) {
		events = append(events, event)
	})
	reopened := mustNewLogForTest(t, root, retainedConfig, "second")
	assertJournalFileCount(t, dir, "archive after second NewLog", 2)
	appendLogEntry(t, reopened, "construction-retention-open", "newlog-retention-on-open", 1_700_002_275_000_010, 10)
	activePath := reopened.ActivePath()
	assertOnlyJournalFile(t, dir, "after lazy open retention", activePath)
	assertLastLifecycleEventType(t, events, LogLifecycleDeleted, "retention deletion")
	verifyJournalctl(t, activePath)
	assertDirectoryJSONRows(t, dir, "TEST_ID=newlog-retention-on-open", "retention-on-open directory", 1)
	closeLogForTest(t, reopened, "second")
}

func TestNewLogEagerRetentionRunsOnOpenForAllPolicies(t *testing.T) {
	requireJournalctl(t)

	for _, tc := range []struct {
		name      string
		retention RetentionPolicy
		artifact  bool
	}{
		{
			name:      "files",
			retention: RetentionPolicy{}.WithMaxFiles(1),
		},
		{
			name:      "bytes",
			retention: RetentionPolicy{}.WithMaxBytes(1),
			artifact:  true,
		},
		{
			name:      "age",
			retention: RetentionPolicy{}.WithMaxAge(time.Microsecond),
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			runEagerRetentionCase(t, tc.name, tc.retention, tc.artifact)
		})
	}
}

func runEagerRetentionCase(t *testing.T, name string, retention RetentionPolicy, useArtifactSizer bool) {
	t.Helper()
	root := t.TempDir()
	config := LogConfig{
		Options:        testOptions(),
		Source:         "system",
		RotationPolicy: RotationPolicy{}.WithMaxEntries(1),
	}
	first := mustNewLogForTest(t, root, config, "first")
	appendLogRange(t, first, "open-retention-"+name, "newlog-eager-retention-on-open", 0, 3, 1_700_002_276_000_000)
	closeLogForTest(t, first, "first")

	dir := filepath.Join(root, config.Options.MachineID.String())
	assertJournalFileCount(t, dir, "archive before eager NewLog", 3)
	time.Sleep(2 * time.Millisecond)

	retainedConfig, events, artifactCalls := eagerRetentionConfig(config, retention, useArtifactSizer)
	reopened := mustNewLogForTest(t, root, retainedConfig, "eager")
	activePath := reopened.ActivePath()
	assertOnlyJournalFile(t, dir, "after eager open retention", activePath)
	assertFirstAndLastLifecycleTypes(t, *events, LogLifecycleCreated, LogLifecycleDeleted, "eager create and retention deletion")
	if useArtifactSizer && len(*artifactCalls) == 0 {
		t.Fatalf("expected artifact sizer calls during open-time byte retention")
	}
	verifyJournalctl(t, activePath)
	closeLogForTest(t, reopened, "eager")
}

func eagerRetentionConfig(config LogConfig, retention RetentionPolicy, useArtifactSizer bool) (LogConfig, *[]LogLifecycleEvent, *[]string) {
	retainedConfig := config
	retainedConfig.RotationPolicy = RotationPolicy{}
	retainedConfig.RetentionPolicy = retention
	retainedConfig.OpenMode = LogOpenEager
	events := []LogLifecycleEvent{}
	artifactCalls := []string{}
	retainedConfig.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) {
		events = append(events, event)
	})
	if useArtifactSizer {
		retainedConfig.ArtifactSizer = LogArtifactSizeFunc(func(path string) (uint64, error) {
			artifactCalls = append(artifactCalls, path)
			return 4096, nil
		})
	}
	return retainedConfig, &events, &artifactCalls
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
