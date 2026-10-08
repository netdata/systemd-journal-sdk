package journal

import (
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

func rootTestConfig() LogConfig {
	return LogConfig{Options: testOptions(), Source: "dem", StrictSystemdNaming: true, RootRetention: true}
}
func rootFixture(t testing.TB, dir string, machine byte, times []uint64, active bool) string {
	t.Helper()
	opts := testOptions()
	opts.MachineID[0] = machine
	opts.FileID = UUID{}
	machineDir := filepath.Join(dir, opts.MachineID.String())
	if err := os.MkdirAll(machineDir, 0700); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(machineDir, "dem.journal")
	w, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	for i, stamp := range times {
		if err = w.Append([]Field{StringField("MESSAGE", fmt.Sprintf("record-%d", i))}, EntryOptions{RealtimeUsec: stamp, MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatal(err)
		}
	}
	if active {
		err = w.Close()
	} else {
		path = filepath.Join(machineDir, rootArchiveName("dem", w.header))
		err = w.ArchiveTo(path)
	}
	if err != nil {
		t.Fatal(err)
	}
	return path
}
func rootOpen(t *testing.T, dir string, cfg LogConfig) *Log {
	t.Helper()
	l, err := NewLog(dir, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = l.CloseWithoutRetention() })
	return l
}
func rootAppend(t *testing.T, l *Log, stamp uint64) {
	t.Helper()
	if err := l.Append([]Field{StringField("MESSAGE", "live")}, EntryOptions{RealtimeUsec: stamp, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
}
func rootInspect(t testing.TB, dir string) RootRetentionInventory {
	t.Helper()
	inv, err := InspectRootRetention(dir, "dem")
	if err != nil {
		t.Fatal(err)
	}
	return inv
}

func TestRootRetentionAcrossMachinesAndTailAge(t *testing.T) {
	dir := t.TempDir()
	now := time.Now()
	day := 24 * time.Hour
	old := uint64(now.Add(-40 * day).UnixMicro())
	young := uint64(now.Add(-20 * day).UnixMicro())
	expired := rootFixture(t, dir, 21, []uint64{old}, false)
	retained := rootFixture(t, dir, 22, []uint64{old, young}, false)
	retired := rootFixture(t, dir, 23, []uint64{old}, true)
	l := rootOpen(t, dir, rootTestConfig())
	if _, err := os.Stat(retired); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("retired active not finalized: %v", err)
	}
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(30 * day)); err != nil {
		t.Fatal(err)
	}
	result, err := l.MaintainRootRetention(now)
	if err != nil {
		t.Fatal(err)
	}
	if result.DeletedFiles != 2 || len(result.Inventory.Files) != 1 || result.Inventory.Files[0].Path != retained {
		t.Fatalf("unexpected result: %+v", result)
	}
	if _, err := os.Stat(expired); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("expired still exists: %v", err)
	}
}
func TestRootRetentionIdleKeepsFileUntilTailExpires(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	cfg.RotationPolicy = RotationPolicy{}.WithMaxDuration(24 * time.Hour)
	l := rootOpen(t, dir, cfg)
	now := time.Now()
	rootAppend(t, l, uint64(now.UnixMicro()))
	path := l.ActivePath()
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(30 * 24 * time.Hour)); err != nil {
		t.Fatal(err)
	}
	for i := 1; i <= 48; i++ {
		r, err := l.MaintainRootRetention(now.Add(time.Duration(i) * time.Hour))
		if err != nil || len(r.Inventory.Files) != 1 || l.ActivePath() != path {
			t.Fatalf("idle sweep %d: %+v %v", i, r, err)
		}
	}
	result, err := l.MaintainRootRetention(now.Add(31 * 24 * time.Hour))
	if err != nil || len(result.Inventory.Files) != 0 || l.ActivePath() != "" {
		t.Fatalf("expiry: %+v %v", result, err)
	}
	rootAppend(t, l, uint64(now.Add(32*24*time.Hour).UnixMicro()))
	if len(rootInspect(t, dir).Files) != 1 {
		t.Fatal("lazy successor missing")
	}
}
func TestRootRetentionFileLengthsAndPolicyChanges(t *testing.T) {
	dir := t.TempDir()
	now := uint64(time.Now().UnixMicro())
	old := rootFixture(t, dir, 21, []uint64{now - 100}, false)
	rootFixture(t, dir, 22, []uint64{now - 50}, false)
	inv := rootInspect(t, dir)
	if len(inv.Files) != 2 || inv.Bytes != 2*8*1024*1024 {
		t.Fatalf("full allocation: %+v", inv)
	}
	l := rootOpen(t, dir, rootTestConfig())
	rootAppend(t, l, now)
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(64 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	r, err := l.MaintainRootRetention(time.Now())
	if err != nil || len(r.Inventory.Files) != 3 {
		t.Fatalf("relax: %+v %v", r, err)
	}
	if err = l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	r, err = l.MaintainRootRetention(time.Now())
	if err != nil || len(r.Inventory.Files) != 1 || r.Inventory.Bytes != 8*1024*1024 {
		t.Fatalf("shrink: %+v %v", r, err)
	}
	if _, err = os.Stat(old); !errors.Is(err, os.ErrNotExist) {
		t.Fatal("oldest survived size pressure")
	}
}
func TestRootRetentionInvalidPolicyAndCopiedOptions(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	cfg.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(64 * 1024 * 1024)
	l := rootOpen(t, dir, cfg)
	rootAppend(t, l, uint64(time.Now().UnixMicro()))
	before := rootInspect(t, dir)
	*cfg.RetentionPolicy.MaxBytes = 1
	if *l.RootRetentionPolicy().MaxBytes != 64*1024*1024 {
		t.Fatal("policy aliased config")
	}
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(0)); err == nil {
		t.Fatal("invalid policy accepted")
	}
	if !reflect.DeepEqual(before, rootInspect(t, dir)) || *l.RootRetentionPolicy().MaxBytes != 64*1024*1024 {
		t.Fatal("invalid policy mutated state")
	}
}
func TestRootInventoryPreservesUnsafeCandidates(t *testing.T) {
	for _, kind := range []string{"quarantine", "seqnum", "machine", "tail", "state", "truncate", "collision", "symlink"} {
		t.Run(kind, func(t *testing.T) {
			dir := t.TempDir()
			path := rootFixture(t, dir, 21, []uint64{1}, false)
			switch kind {
			case "quarantine":
				if err := os.Rename(path, path+"~"); err != nil {
					t.Fatal(err)
				}
			case "seqnum":
				name := filepath.Base(path)
				if err := os.Rename(path, filepath.Join(filepath.Dir(path), name[:4]+"ffffffffffffffffffffffffffffffff"+name[36:])); err != nil {
					t.Fatal(err)
				}
			case "machine", "tail", "state":
				f, err := os.OpenFile(path, os.O_RDWR, 0)
				if err != nil {
					t.Fatal(err)
				}
				offset := int64(40)
				data := []byte{99}
				if kind == "tail" {
					offset = 192
					data = make([]byte, 8)
				}
				if kind == "state" {
					offset = 16
					data = []byte{stateOnline}
				}
				_, err = f.WriteAt(data, offset)
				_ = f.Close()
				if err != nil {
					t.Fatal(err)
				}
			case "truncate":
				if err := os.Truncate(path, headerSize); err != nil {
					t.Fatal(err)
				}
			case "collision":
				data, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				if err = os.WriteFile(filepath.Join(filepath.Dir(path), "dem.journal"), data, 0600); err != nil {
					t.Fatal(err)
				}
			case "symlink":
				if err := os.Symlink(path, filepath.Join(filepath.Dir(path), "dem.journal")); err != nil {
					t.Fatal(err)
				}
			}
			before, _ := filepath.Glob(filepath.Join(dir, "*", "*"))
			if _, err := InspectRootRetention(dir, "dem"); err == nil {
				t.Fatal("unsafe inventory accepted")
			}
			cfg := rootTestConfig()
			cfg.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(1)
			if _, err := NewLog(dir, cfg); err == nil {
				t.Fatal("unsafe root opened")
			}
			after, _ := filepath.Glob(filepath.Join(dir, "*", "*"))
			if !reflect.DeepEqual(before, after) {
				t.Fatalf("evidence changed: %v / %v", before, after)
			}
		})
	}
}
func TestRootRetentionCleanupFailureDoesNotFailAppend(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	cfg.RotationPolicy = RotationPolicy{}.WithMaxEntries(1)
	cfg.RetentionPolicy = RetentionPolicy{}.WithMaxFiles(1)
	l := rootOpen(t, dir, cfg)
	stamp := uint64(time.Now().UnixMicro())
	rootAppend(t, l, stamp)
	original := removeRootRetentionFile
	failure := errors.New("synthetic unlink failure")
	removeRootRetentionFile = func(string) error { return failure }
	defer func() { removeRootRetentionFile = original }()
	rootAppend(t, l, stamp+1)
	r, ok := l.LastRootRetentionResult()
	if !ok || !errors.Is(r.Err, failure) || !r.InventoryValid || r.LastSuccessfulAt.IsZero() {
		t.Fatalf("cleanup outcome: %+v %v", r, ok)
	}
	if err := l.Sync(); err != nil {
		t.Fatalf("writer poisoned: %v", err)
	}
	snap, err := OpenIndexedSnapshot(context.Background(), l.ActivePath(), IndexedSnapshotOptions{})
	if err != nil {
		t.Fatal(err)
	}
	defer snap.Close()
	if snap.EntryCount() != 1 {
		t.Fatalf("append not saved: %d", snap.EntryCount())
	}
	removeRootRetentionFile = original
	if _, err := l.MaintainRootRetention(time.Now()); err != nil {
		t.Fatal(err)
	}
}
func TestRootRetentionCapturedReaderSurvivesUnlink(t *testing.T) {
	dir := t.TempDir()
	path := rootFixture(t, dir, 21, []uint64{1, 2}, false)
	snap, err := OpenIndexedSnapshot(context.Background(), path, IndexedSnapshotOptions{})
	if err != nil {
		t.Fatal(err)
	}
	defer snap.Close()
	l := rootOpen(t, dir, rootTestConfig())
	if err = l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(time.Hour)); err != nil {
		t.Fatal(err)
	}
	if _, err = l.MaintainRootRetention(time.Now()); err != nil {
		t.Fatal(err)
	}
	var seqs []uint64
	err = snap.VisitEntries(context.Background(), func(e *SnapshotEntry) error { seqs = append(seqs, e.Seqnum); return nil })
	if err != nil || !reflect.DeepEqual(seqs, []uint64{1, 2}) {
		t.Fatalf("captured entries: %v %v", seqs, err)
	}
}
func TestRootInventoryMetadataOnly(t *testing.T) {
	dir := t.TempDir()
	path := rootFixture(t, dir, 21, []uint64{1}, false)
	h, err := readJournalHeader(path)
	if err != nil {
		t.Fatal(err)
	}
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	bad := make([]byte, 8)
	binary.LittleEndian.PutUint64(bad, ^uint64(0))
	_, err = f.WriteAt(bad, int64(h.entryArrayOffset+offsetArrayObjectHeaderSize))
	_ = f.Close()
	if err != nil {
		t.Fatal(err)
	}
	if len(rootInspect(t, dir).Files) != 1 {
		t.Fatal("header metadata unavailable")
	}
}
func BenchmarkInspectRootRetention(b *testing.B) {
	for _, records := range []int{1, 10000} {
		b.Run(fmt.Sprintf("records_%d", records), func(b *testing.B) {
			dir := b.TempDir()
			times := make([]uint64, records)
			for i := range times {
				times[i] = uint64(i + 1)
			}
			rootFixture(b, dir, 21, times, false)
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				if _, err := InspectRootRetention(dir, "dem"); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

func TestRootInventoryMetadataDirectoriesAndMissingRoot(t *testing.T) {
	dir := t.TempDir()
	if err := os.Mkdir(filepath.Join(dir, "identity"), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "identity", "metadata"), []byte("synthetic"), 0600); err != nil {
		t.Fatal(err)
	}
	if len(rootInspect(t, dir).Files) != 0 {
		t.Fatal("metadata counted")
	}
	if _, err := InspectRootRetention(filepath.Join(dir, "missing"), "dem"); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("missing root: %v", err)
	}
	path := rootFixture(t, dir, 21, []uint64{1}, false)
	if err := os.Rename(filepath.Dir(path), filepath.Join(dir, "invalid-machine")); err != nil {
		t.Fatal(err)
	}
	if _, err := InspectRootRetention(dir, "dem"); err == nil {
		t.Fatal("noncanonical owner accepted")
	}
}

func TestRootRetentionEmptyPolicyTransition(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	cfg.OpenMode = LogOpenEager
	cfg.RetentionPolicy = RetentionPolicy{}.WithMaxBytes(1024 * 1024 * 1024)
	l := rootOpen(t, dir, cfg)
	if len(rootInspect(t, dir).Files) != 1 {
		t.Fatal("eager file absent")
	}
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	result, err := l.MaintainRootRetention(time.Now())
	if err != nil || len(result.Inventory.Files) != 0 || l.ActivePath() != "" {
		t.Fatalf("empty transition: %+v %v", result, err)
	}
	rootAppend(t, l, uint64(time.Now().UnixMicro()))
	if len(rootInspect(t, dir).Files) != 1 {
		t.Fatal("next file not lazy")
	}
}

func TestRootRetentionFixedSpanRotation(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	cfg.RotationPolicy = RotationPolicy{}.WithMaxDuration(24 * time.Hour)
	l := rootOpen(t, dir, cfg)
	stamp := uint64(time.Now().UnixMicro())
	rootAppend(t, l, stamp)
	rootAppend(t, l, stamp+uint64((24*time.Hour-time.Microsecond)/time.Microsecond))
	if len(rootInspect(t, dir).Files) != 1 {
		t.Fatal("early rotation")
	}
	rootAppend(t, l, stamp+uint64(24*time.Hour/time.Microsecond))
	inv := rootInspect(t, dir)
	if len(inv.Files) != 2 {
		t.Fatal("span boundary did not rotate")
	}
	for _, f := range inv.Files {
		if f.TailRealtime-f.HeadRealtime >= uint64(24*time.Hour/time.Microsecond) {
			t.Fatalf("span exceeds bound: %+v", f)
		}
	}
}

func TestRootRetentionArchiveFailureStopsWritesAndPruning(t *testing.T) {
	dir := t.TempDir()
	cfg := rootTestConfig()
	l := rootOpen(t, dir, cfg)
	now := time.Now()
	rootAppend(t, l, uint64(now.UnixMicro()))
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(time.Hour)); err != nil {
		t.Fatal(err)
	}
	oldSync := syncArchiveJournalFile
	fault := errors.New("synthetic archive sync failure")
	syncArchiveJournalFile = func(*Writer) error { return fault }
	defer func() { syncArchiveJournalFile = oldSync }()
	_, err := l.MaintainRootRetention(now.Add(2 * time.Hour))
	if !errors.Is(err, ErrWriterFailed) || !errors.Is(err, fault) {
		t.Fatalf("archive failure: %v", err)
	}
	if err = l.Append([]Field{StringField("MESSAGE", "must fail")}, EntryOptions{MonotonicUsec: 2}); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("write after archive failure: %v", err)
	}
	files, _ := filepath.Glob(filepath.Join(dir, "*", "dem*"))
	if len(files) != 1 {
		t.Fatalf("uncertain evidence discarded: %v", files)
	}
}

func TestRootRetentionRejectsNewUnsafeCandidateBeforePruning(t *testing.T) {
	dir := t.TempDir()
	path := rootFixture(t, dir, 21, []uint64{1}, false)
	l := rootOpen(t, dir, rootTestConfig())
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(time.Hour)); err != nil {
		t.Fatal(err)
	}
	bad := filepath.Join(filepath.Dir(path), "dem@uncertain.journal~")
	if err := os.WriteFile(bad, []byte("evidence"), 0600); err != nil {
		t.Fatal(err)
	}
	result, err := l.MaintainRootRetention(time.Now())
	if err == nil || result.InventoryValid {
		t.Fatalf("unsafe root: %+v %v", result, err)
	}
	if _, err = os.Stat(path); err != nil {
		t.Fatalf("known archive pruned before validation: %v", err)
	}
}

func BenchmarkRootRetentionMaintenance(b *testing.B) {
	for _, count := range []int{1, 20} {
		b.Run(fmt.Sprintf("files_%d", count), func(b *testing.B) {
			dir := b.TempDir()
			for i := 0; i < count; i++ {
				rootFixture(b, dir, byte(i+40), []uint64{uint64(i + 1)}, false)
			}
			l, err := NewLog(dir, rootTestConfig())
			if err != nil {
				b.Fatal(err)
			}
			defer l.CloseWithoutRetention()
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				if _, err := l.MaintainRootRetention(time.Now()); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

func TestRootRetentionPolicyBeforeFirstLazyAppend(t *testing.T) {
	dir := t.TempDir()
	l := rootOpen(t, dir, rootTestConfig())
	if err := l.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxBytes(8 * 1024 * 1024)); err != nil {
		t.Fatal(err)
	}
	rootAppend(t, l, uint64(time.Now().UnixMicro()))
	inv := rootInspect(t, dir)
	if len(inv.Files) != 1 || inv.Files[0].Entries != 1 {
		t.Fatalf("first append: %+v", inv)
	}
}

func BenchmarkRootRetentionRotation(b *testing.B) {
	for _, enabled := range []bool{false, true} {
		b.Run(fmt.Sprintf("root_%t", enabled), func(b *testing.B) {
			cfg := rootTestConfig()
			cfg.RootRetention = enabled
			cfg.RotationPolicy = RotationPolicy{}.WithMaxEntries(1)
			cfg.RetentionPolicy = RetentionPolicy{}.WithMaxFiles(2)
			l, err := NewLog(b.TempDir(), cfg)
			if err != nil {
				b.Fatal(err)
			}
			defer l.CloseWithoutRetention()
			fields := []Field{StringField("MESSAGE", "rotation")}
			start := uint64(time.Now().UnixMicro())
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				if err := l.Append(fields, EntryOptions{RealtimeUsec: start + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}
