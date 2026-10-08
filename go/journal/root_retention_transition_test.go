package journal_test

import (
	"encoding/binary"
	"errors"
	"os"
	"testing"
	"time"

	journal "github.com/netdata/systemd-journal-sdk/go/journal"
)

func transitionConfig(t *testing.T) journal.LogConfig {
	t.Helper()
	machine, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
	if err != nil {
		t.Fatal(err)
	}
	boot, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
	if err != nil {
		t.Fatal(err)
	}
	return journal.LogConfig{Source: "history", StrictSystemdNaming: true, RootRetention: true, Options: journal.Options{MachineID: machine, BootID: boot}}
}
func transitionAppend(t *testing.T, l *journal.Log, raw bool) {
	t.Helper()
	var err error
	if raw {
		err = l.AppendRaw([][]byte{[]byte("MESSAGE=record")}, journal.EntryOptions{MonotonicUsec: 1})
	} else {
		err = l.Append([]journal.Field{journal.StringField("MESSAGE", "record")}, journal.EntryOptions{MonotonicUsec: 1})
	}
	if err != nil {
		t.Fatal(err)
	}
}
func transitionGeometry(t *testing.T, path string) (int64, uint64) {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	var header [272]byte
	if _, err = f.ReadAt(header[:], 0); err != nil {
		t.Fatal(err)
	}
	info, err := f.Stat()
	if err != nil {
		t.Fatal(err)
	}
	return info.Size(), binary.LittleEndian.Uint64(header[112:120])
}

func TestRootRetentionTransitionRebuildsDerivedGeometry(t *testing.T) {
	for _, tc := range []struct {
		name    string
		maxSize uint64
		buckets int
	}{
		{name: "derived"}, {name: "explicit_buckets", buckets: 100000}, {name: "explicit_file_size", maxSize: 128 * 1024 * 1024},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cfg := transitionConfig(t)
			cfg.Options.MaxFileSize = tc.maxSize
			cfg.Options.DataHashTableBuckets = tc.buckets
			cfg.RetentionPolicy = journal.RetentionPolicy{}.WithMaxBytes(32 * 1024 * 1024 * 1024)
			l, err := journal.NewLog(t.TempDir(), cfg)
			if err != nil {
				t.Fatal(err)
			}
			defer l.CloseWithoutRetention()
			transitionAppend(t, l, false)
			_, before := transitionGeometry(t, l.ActivePath())
			if err = l.SetRootRetentionPolicy(journal.RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
				t.Fatal(err)
			}
			if _, err = l.MaintainRootRetention(time.Now()); err != nil {
				t.Fatal(err)
			}
			transitionAppend(t, l, false)
			size, after := transitionGeometry(t, l.ActivePath())
			if size != 8*1024*1024 {
				t.Fatalf("successor file length = %d, want 8388608 (data hash bytes %d)", size, after)
			}
			if tc.maxSize != 0 || tc.buckets != 0 {
				if after != before {
					t.Fatalf("caller geometry changed: %d -> %d", before, after)
				}
			} else if after >= before {
				t.Fatalf("derived geometry did not shrink: %d -> %d", before, after)
			}
		})
	}
}

func TestRootRetentionTransitionMaintainsEveryLazySuccessor(t *testing.T) {
	for _, raw := range []bool{false, true} {
		name := "Append"
		if raw {
			name = "AppendRaw"
		}
		t.Run(name, func(t *testing.T) {
			dir := t.TempDir()
			cfg := transitionConfig(t)
			cfg.RetentionPolicy = journal.RetentionPolicy{}.WithMaxBytes(1024 * 1024 * 1024)
			l, err := journal.NewLog(dir, cfg)
			if err != nil {
				t.Fatal(err)
			}
			defer l.CloseWithoutRetention()
			transitionAppend(t, l, raw)
			if err = l.SetRootRetentionPolicy(journal.RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
				t.Fatal(err)
			}
			result, err := l.MaintainRootRetention(time.Now().Add(-time.Second))
			if err != nil {
				t.Fatal(err)
			}
			if len(result.Inventory.Files) != 1 || l.ActivePath() != "" {
				t.Fatalf("maintenance did not leave one archive and lazy successor: %+v", result)
			}
			transitionAppend(t, l, raw)
			inv, err := journal.InspectRootRetention(dir, "history")
			if err != nil {
				t.Fatal(err)
			}
			if len(inv.Files) != 1 || inv.Bytes != 8*1024*1024 || !inv.Files[0].Active || inv.Files[0].Entries != 1 {
				t.Fatalf("successor creation skipped cleanup: %+v", inv)
			}
			last, ok := l.LastRootRetentionResult()
			if !ok || !last.AttemptedAt.After(result.AttemptedAt) || last.Err != nil {
				t.Fatalf("successor maintenance outcome missing: %+v", last)
			}
		})
	}
}

func TestDefaultLogRotationCleanupFailureKeepsAppendRetrySemantics(t *testing.T) {
	cfg := transitionConfig(t)
	cfg.RootRetention = false
	cfg.RotationPolicy = journal.RotationPolicy{}.WithMaxEntries(1)
	failing := false
	fault := errors.New("synthetic default artifact-size failure")
	cfg.ArtifactSizer = journal.LogArtifactSizeFunc(func(string) (uint64, error) {
		if failing {
			return 0, fault
		}
		return 0, nil
	})
	l, err := journal.NewLog(t.TempDir(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer l.CloseWithoutRetention()
	transitionAppend(t, l, false)
	failing = true
	err = l.Append([]journal.Field{journal.StringField("MESSAGE", "retry")}, journal.EntryOptions{MonotonicUsec: 2})
	if !errors.Is(err, fault) {
		t.Fatalf("rotation cleanup did not fail: %v", err)
	}
	// The default API has already attempted rotation cleanup. Retrying the
	// append uses its empty successor even while the same cleanup error persists.
	transitionAppend(t, l, false)
}

func TestRootRetentionTransitionRevertedPolicyKeepsLiveGeometry(t *testing.T) {
	dir := t.TempDir()
	cfg := transitionConfig(t)
	large := journal.RetentionPolicy{}.WithMaxBytes(32 * 1024 * 1024 * 1024)
	cfg.RetentionPolicy = large
	l, err := journal.NewLog(dir, cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer l.CloseWithoutRetention()
	transitionAppend(t, l, false)
	path := l.ActivePath()
	if err = l.SetRootRetentionPolicy(journal.RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	if err = l.SetRootRetentionPolicy(large); err != nil {
		t.Fatal(err)
	}
	result, err := l.MaintainRootRetention(time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if l.ActivePath() != path || len(result.Inventory.Files) != 1 || !result.Inventory.Files[0].Active {
		t.Fatalf("reverted policy fragmented live file: %+v", result)
	}
}
