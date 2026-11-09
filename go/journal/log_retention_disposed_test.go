package journal

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// Paired with Rust retention_counts_and_deletes_disposed_files_first.
func TestRetentionCountsAndDeletesDisposedFilesFirst(t *testing.T) {
	root := t.TempDir()
	log := mustNewLogForTest(t, root, LogConfig{
		Options:         testOptions(),
		Source:          "system",
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(2),
	}, "retention")
	defer closeLogForTest(t, log, "retention")
	appendLogRange(t, log, "keep", "", 0, 2, 1_700_002_900_000_000)
	dir := log.JournalDirectory()

	// Newer than every archive, to prove disposed files still go first.
	stamp := uint64(time.Now().UnixMicro())
	chainName := fmt.Sprintf("system@%032x-%016x-%016x@%016x-%016x.journal~", 1, 1, 1, stamp, 2)
	strictName := fmt.Sprintf("system@%016x-%016x.journal~", stamp, 1)
	foreign := fmt.Sprintf("other@%016x-%016x.journal~", stamp, 1)
	for _, name := range []string{chainName, strictName, foreign} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := log.EnforceRetention(); err != nil {
		t.Fatal(err)
	}
	assertPathDoesNotExist(t, filepath.Join(dir, chainName), "chain disposed")
	assertPathDoesNotExist(t, filepath.Join(dir, strictName), "strict disposed")
	if _, err := os.Stat(filepath.Join(dir, foreign)); err != nil {
		t.Fatalf("another source's file was touched: %v", err)
	}
	if _, err := os.Stat(log.ActivePath()); err != nil {
		t.Fatalf("active file: %v", err)
	}
}

// A recent disposed file sorts first, and must not stop older archives aging out.
func TestRetentionMaxAgeSkipsRecentDisposedFile(t *testing.T) {
	now := uint64(time.Now().UnixMicro())
	log := &Log{}
	maxAge := time.Hour
	log.retention.MaxAge = &maxAge
	dir := t.TempDir()
	recent := filepath.Join(dir, "recent")
	old := filepath.Join(dir, "old")
	for _, path := range []string{recent, old} {
		if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	run := retentionRun{files: []archivedJournalFile{
		{path: recent, headRealtime: now, disposed: true},
		{path: old, headRealtime: now - uint64(2*time.Hour/time.Microsecond)},
	}, fileCount: 2}
	if err := log.enforceRetentionMaxAge(&run); err != nil {
		t.Fatal(err)
	}
	assertPathDoesNotExist(t, old, "expired archive")
	if _, err := os.Stat(recent); err != nil {
		t.Fatalf("recent disposed file: %v", err)
	}
}
