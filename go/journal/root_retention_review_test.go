package journal

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestRootRetentionEmptyRemovalCountSurvivesSyncFailure(t *testing.T) {
	for _, live := range []bool{false, true} {
		t.Run(map[bool]string{false: "retired", true: "live"}[live], func(t *testing.T) {
			dir := t.TempDir()
			cfg := rootTestConfig()
			cfg.Options.DataHashTableBuckets = 0
			cfg.OpenMode = LogOpenEager
			cfg.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(1 << 30)
			log := rootOpen(t, dir, cfg)
			path := log.ActivePath()
			if live {
				if err := log.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(8 << 20)); err != nil {
					t.Fatal(err)
				}
			} else {
				path = rootFixture(t, dir, 23, nil, true)
			}
			last, _ := log.LastRootRetentionResult()
			fault := errors.New("synthetic empty removal directory sync failure")
			original := syncJournalDirectory
			syncJournalDirectory = func(dir string) error {
				if dir == filepath.Dir(path) {
					return fault
				}
				return original(dir)
			}
			t.Cleanup(func() { syncJournalDirectory = original })
			result, err := log.MaintainRootRetention(time.Now())
			if !errors.Is(err, fault) || errors.Is(err, ErrWriterFailed) {
				t.Fatalf("expected safe sync failure, got %v", err)
			}
			if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("file was not removed: %v", err)
			}
			stored, ok := log.LastRootRetentionResult()
			if !ok || result.DeletedFiles != 1 || stored.DeletedFiles != 1 {
				t.Fatalf("lost successful removal: result=%+v stored=%+v", result, stored)
			}
			if result.InventoryValid || !result.LastSuccessfulAt.Equal(last.LastSuccessfulAt) {
				t.Fatalf("failed maintenance reported success: %+v", result)
			}
			syncJournalDirectory = original
			rootAppend(t, log, uint64(time.Now().UnixMicro()))
			if _, err := log.MaintainRootRetention(time.Now()); err != nil {
				t.Fatal(err)
			}
		})
	}
}
