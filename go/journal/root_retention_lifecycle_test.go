package journal

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func TestRootRetentionArchiveLifecycle(t *testing.T) {
	for _, boundary := range []string{"retired", "policy", "expiry", "close", "close_without_retention"} {
		t.Run(boundary, func(t *testing.T) {
			dir := t.TempDir()
			now := time.Now()
			cfg := rootTestConfig()
			cfg.Options.DataHashTableBuckets = 0
			cfg.Options.FieldHashTableBuckets = 0
			cfg.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(32 << 30)
			var events []LogLifecycleEvent
			cfg.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) {
				if event.Type == LogLifecycleArchived {
					if event.ActivePath != "" || event.ArchivedPath == "" {
						t.Errorf("archive without successor has invalid paths: %+v", event)
					}
					if _, err := os.Stat(event.ArchivedPath); err != nil {
						t.Errorf("archive event must precede deletion: %v", err)
					}
					h, err := readJournalHeader(event.ArchivedPath)
					if err != nil || h.state != stateArchived {
						t.Errorf("archive event before finalization: header=%+v err=%v", h, err)
					}
				}
				events = append(events, event)
			})
			if boundary == "retired" {
				rootFixture(t, dir, 23, []uint64{uint64(now.UnixMicro())}, true)
			}
			log := rootOpen(t, dir, cfg)
			if boundary != "retired" {
				rootAppend(t, log, uint64(now.UnixMicro()))
				events = nil
			}
			var err error
			switch boundary {
			case "policy":
				err = log.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(1 << 30))
				if err == nil {
					_, err = log.MaintainRootRetention(now)
				}
			case "expiry":
				err = log.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(time.Hour))
				if err == nil {
					_, err = log.MaintainRootRetention(now.Add(2 * time.Hour))
				}
			case "close":
				err = log.Close()
			case "close_without_retention":
				err = log.CloseWithoutRetention()
			}
			if err != nil {
				t.Fatal(err)
			}
			archives := 0
			for _, event := range events {
				switch event.Type {
				case LogLifecycleArchived:
					archives++
					if event.Reason != LogLifecycleReasonRetention {
						t.Errorf("archive reason: %+v", event)
					}
				case LogLifecycleRotated, LogLifecycleCreated:
					t.Errorf("finalization created/announced a successor: %+v", event)
				}
			}
			if archives != 1 {
				t.Fatalf("want one archive event, got %+v", events)
			}
		})
	}
}

func TestRootRetentionResultDoesNotAliasStoredStatus(t *testing.T) {
	dir := t.TempDir()
	path := rootFixture(t, dir, 21, []uint64{1}, false)
	log := rootOpen(t, dir, rootTestConfig())
	result, err := log.MaintainRootRetention(time.Now())
	if err != nil {
		t.Fatal(err)
	}
	result.Inventory.Files[0].Path = "caller-edit"
	stored, ok := log.LastRootRetentionResult()
	if !ok || stored.Inventory.Files[0].Path != path {
		t.Fatalf("returned result changed status: %+v", stored)
	}
	stored.Inventory.Files[0].Path = "second-caller-edit"
	again, _ := log.LastRootRetentionResult()
	if again.Inventory.Files[0].Path != path {
		t.Fatal("status accessor changed retained result")
	}
}

func TestRootRetentionRetiredOpenPermissionFailureIsRecoverable(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Unix permission-mode denial")
	}
	dir := t.TempDir()
	now := time.Now()
	log := rootOpen(t, dir, rootTestConfig())
	rootAppend(t, log, uint64(now.UnixMicro()))
	path := rootFixture(t, dir, 23, []uint64{uint64(now.UnixMicro())}, true)
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0400); err != nil {
		t.Fatal(err)
	}
	defer os.Chmod(path, 0600)
	if f, err := os.OpenFile(path, os.O_RDWR, 0); err == nil {
		f.Close()
		t.Skip("process can write a read-only file")
	} else if !errors.Is(err, os.ErrPermission) {
		t.Fatal(err)
	}
	_, err = log.MaintainRootRetention(now)
	if !errors.Is(err, os.ErrPermission) || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("safe retired-open failure: %v", err)
	}
	after, err := os.ReadFile(path)
	if err != nil || !bytes.Equal(before, after) {
		t.Fatalf("retired evidence changed: %v", err)
	}
	rootAppend(t, log, uint64(now.Add(time.Second).UnixMicro()))
	if _, err := log.InspectRootRetention(); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0600); err != nil {
		t.Fatal(err)
	}
	result, err := log.MaintainRootRetention(now.Add(time.Second))
	if err != nil || !result.InventoryValid || len(result.Inventory.Files) != 2 {
		t.Fatalf("permission recovery: %+v %v", result, err)
	}
}

func TestDefaultRetentionAfterFailedSuccessorCreation(t *testing.T) {
	cfg := LogConfig{Options: testOptions(), Source: "dem", RotationPolicy: RotationPolicy{}.WithMaxEntries(1), RetentionPolicy: RetentionPolicy{}.WithMaxFiles(1)}
	log, err := NewLog(t.TempDir(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer log.CloseWithoutRetention()
	now := uint64(time.Now().UnixMicro())
	rootAppend(t, log, now)
	first := log.ActivePath()
	blocked := log.chainPathFor(log.options.SeqnumID, 2, now+10)
	if err := os.Mkdir(blocked, 0700); err != nil {
		t.Fatal(err)
	}
	opts := EntryOptions{RealtimeUsec: now + 10, MonotonicUsec: 2}
	if err := log.Append([]Field{StringField("MESSAGE", "next")}, opts); err == nil || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("want safe successor creation failure: %v", err)
	}
	if err := os.Remove(blocked); err != nil {
		t.Fatal(err)
	}
	if err := log.Append([]Field{StringField("MESSAGE", "retry")}, opts); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(first); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("successful creation did not enforce retention: %v", err)
	}
}

func TestAppendOpenMutationFailureClass(t *testing.T) {
	path := rootFixture(t, t.TempDir(), 23, []uint64{1}, true)
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	// The read-only descriptor passes header validation but cannot enter the
	// writable mapping stage, which can resize a file before failing.
	if w, err := newAppendWriter(path, f, Options{}); !errors.Is(err, ErrWriterFailed) {
		if w != nil {
			w.Close()
		}
		t.Fatalf("mapping-stage failure not classified as uncertain: %v", err)
	}
	_, err = OpenWithOptions(filepath.Join(t.TempDir(), "absent.journal"), Options{})
	if !errors.Is(err, os.ErrNotExist) || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("nonmutating open failure classified as uncertain: %v", err)
	}
}
