package journal_test

import (
	"encoding/binary"
	"errors"
	"os"
	"path/filepath"
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

func TestRootRetentionMaintenanceDetectsMissingLiveDirectory(t *testing.T) {
	for _, kind := range []string{"directory", "file"} {
		t.Run(kind, func(t *testing.T) {
			root := t.TempDir()
			cfg := transitionConfig(t)
			l, err := journal.NewLog(root, cfg)
			if err != nil {
				t.Fatal(err)
			}
			defer l.CloseWithoutRetention()
			transitionAppend(t, l, false)
			original := l.JournalDirectory()
			if kind == "file" {
				original = l.ActivePath()
			}
			moved := filepath.Join(t.TempDir(), "moved-live-path")
			if err = os.Rename(original, moved); err != nil {
				t.Fatal(err)
			}
			away := true
			defer func() {
				if away {
					if err := os.Rename(moved, original); err != nil {
						t.Error(err)
					}
				}
			}()
			if _, err = l.InspectRootRetention(); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("missing live inventory: %v", err)
			}
			previous, _ := l.LastRootRetentionResult()
			result, err := l.MaintainRootRetention(time.Now())
			if !errors.Is(err, os.ErrNotExist) || result.InventoryValid || errors.Is(err, journal.ErrWriterFailed) {
				t.Fatalf("missing live path: valid=%t err=%v", result.InventoryValid, err)
			}
			if !result.LastSuccessfulAt.Equal(previous.LastSuccessfulAt) {
				t.Fatal("failed inventory replaced last maintenance success")
			}
			if err = l.Sync(); err != nil {
				t.Fatalf("inventory error poisoned writer: %v", err)
			}
			if err = os.Rename(moved, original); err != nil {
				t.Fatal(err)
			}
			away = false
			inventory, err := l.InspectRootRetention()
			if err != nil || len(inventory.Files) != 1 {
				t.Fatalf("recovered inventory: files=%d err=%v", len(inventory.Files), err)
			}
			result, err = l.MaintainRootRetention(time.Now())
			if err != nil || !result.InventoryValid {
				t.Fatalf("recovered maintenance: %v", err)
			}
			transitionAppend(t, l, false)
		})
	}
}

func TestRootRetentionRejectsMachineNamedRegularFile(t *testing.T) {
	root := t.TempDir()
	cfg := transitionConfig(t)
	if err := os.WriteFile(filepath.Join(root, cfg.Options.MachineID.String()), []byte("not a directory"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := journal.InspectRootRetention(root, cfg.Source); err == nil {
		t.Fatal("machine-named regular file accepted as empty inventory")
	}
}

func TestRootRetentionLiveInventoryRejectsReplacementFile(t *testing.T) {
	root := t.TempDir()
	l, err := journal.NewLog(root, transitionConfig(t))
	if err != nil {
		t.Fatal(err)
	}
	defer l.CloseWithoutRetention()
	transitionAppend(t, l, false)
	original := l.ActivePath()
	holding := t.TempDir()
	moved := filepath.Join(holding, "live")
	copyPath := filepath.Join(holding, "copy")
	bytes, err := os.ReadFile(original)
	if err != nil {
		t.Fatal(err)
	}
	if err = os.Rename(original, moved); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if _, err := os.Stat(original); err == nil {
			if err := os.Rename(original, copyPath); err != nil {
				t.Error(err)
			}
		}
		if err := os.Rename(moved, original); err != nil {
			t.Error(err)
		}
	}()
	if err = os.WriteFile(original, bytes, 0600); err != nil {
		t.Fatal(err)
	}
	// A copied valid header has identical journal IDs but is not the live inode.
	if _, err = journal.InspectRootRetention(root, "history"); err != nil {
		t.Fatalf("copy should pass standalone headers: %v", err)
	}
	if _, err = l.InspectRootRetention(); err == nil || errors.Is(err, journal.ErrWriterFailed) {
		t.Fatalf("replacement inventory: %v", err)
	}
	result, err := l.MaintainRootRetention(time.Now())
	if err == nil || result.InventoryValid || errors.Is(err, journal.ErrWriterFailed) {
		t.Fatalf("replacement maintenance: valid=%t err=%v", result.InventoryValid, err)
	}
	if err = l.Sync(); err != nil {
		t.Fatalf("read-only mismatch poisoned writer: %v", err)
	}
}

func TestRootRetentionLiveInventoryRejectsChangedHeaderIdentity(t *testing.T) {
	root := t.TempDir()
	l, err := journal.NewLog(root, transitionConfig(t))
	if err != nil {
		t.Fatal(err)
	}
	defer l.CloseWithoutRetention()
	transitionAppend(t, l, false)
	f, err := os.OpenFile(l.ActivePath(), os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	var fileID [16]byte
	if _, err = f.ReadAt(fileID[:], 24); err != nil {
		t.Fatal(err)
	}
	changed := fileID
	changed[0] ^= 1
	if _, err = f.WriteAt(changed[:], 24); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if _, err := f.WriteAt(fileID[:], 24); err != nil {
			t.Error(err)
		}
	}()
	if _, err = l.InspectRootRetention(); err == nil || errors.Is(err, journal.ErrWriterFailed) {
		t.Fatalf("changed identity accepted: %v", err)
	}
	if _, err = f.WriteAt(fileID[:], 24); err != nil {
		t.Fatal(err)
	}
	if _, err = l.InspectRootRetention(); err != nil {
		t.Fatalf("restored identity rejected: %v", err)
	}
}

func TestRootRetentionLiveInventoryRequiresOptInAndOpenLog(t *testing.T) {
	cfg := transitionConfig(t)
	cfg.RootRetention = false
	legacy, err := journal.NewLog(t.TempDir(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer legacy.CloseWithoutRetention()
	if _, err = legacy.InspectRootRetention(); err == nil {
		t.Fatal("non-opt-in inspection accepted")
	}
	cfg.RootRetention = true
	l, err := journal.NewLog(t.TempDir(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = l.InspectRootRetention(); err != nil {
		t.Fatalf("open lazy inspection: %v", err)
	}
	if err = l.CloseWithoutRetention(); err != nil {
		t.Fatal(err)
	}
	if _, err = l.InspectRootRetention(); err == nil {
		t.Fatal("closed Log inspection accepted")
	}
}
