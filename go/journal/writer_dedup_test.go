package journal

import (
	"errors"
	"path/filepath"
	"testing"
)

func TestHashCollisionChainsDeduplicate(t *testing.T) {
	path := filepath.Join(t.TempDir(), "hash-collisions.journal")
	opts := testOptions()
	opts.DataHashTableBuckets = 1
	opts.FieldHashTableBuckets = 1

	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	entries := [][]Field{
		{StringField("MESSAGE", "one"), StringField("PRIORITY", "6")},
		{StringField("MESSAGE", "two"), StringField("PRIORITY", "6")},
		{StringField("MESSAGE", "one"), StringField("PRIORITY", "6")},
	}
	for i, fields := range entries {
		if err := w.Append(fields, EntryOptions{RealtimeUsec: 1_700_000_020_000_000 + uint64(i), MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append(%d) error = %v", i, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	snapshot := readJournalSnapshot(t, path)
	if got := len(snapshot.dataByPayload); got != 3 {
		t.Fatalf("unique data object count = %d, want 3", got)
	}
	if got := len(snapshot.fieldByPayload); got != 2 {
		t.Fatalf("unique field object count = %d, want 2", got)
	}
	if got := snapshot.dataByPayload["MESSAGE=one"].header.nEntries; got != 2 {
		t.Fatalf("MESSAGE=one nEntries = %d, want 2", got)
	}
}

func TestWriterSyncCloseAndClosedAppend(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sync-close.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "sync")}, testEntryOptions(1)); err != nil {
		t.Fatalf("Append() error = %v", err)
	}
	if err := w.Sync(); err != nil {
		t.Fatalf("Sync() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("second Close() error = %v", err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "after close")}, testEntryOptions(1)); !errors.Is(err, errWriterClosed) {
		t.Fatalf("Append(after Close) error = %v, want errWriterClosed", err)
	}
	if err := w.Sync(); !errors.Is(err, errWriterClosed) {
		t.Fatalf("Sync(after Close) error = %v, want errWriterClosed", err)
	}
}

func TestAppendMapUsesDeterministicOrdering(t *testing.T) {
	path := filepath.Join(t.TempDir(), "map.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.AppendMapWithOptions(map[string]string{
		"SYSLOG_IDENTIFIER": "go-test",
		"PRIORITY":          "6",
		"MESSAGE":           "ordered",
	}, testEntryOptions(1)); err != nil {
		t.Fatalf("AppendMapWithOptions() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	snapshot := readJournalSnapshot(t, path)
	if len(snapshot.entries) != 1 {
		t.Fatalf("entry count = %d, want 1", len(snapshot.entries))
	}

	gotOffsets := snapshot.entries[0].itemOffsets
	wantOffsets := append([]uint64(nil), gotOffsets...)
	for i := 1; i < len(wantOffsets); i++ {
		if wantOffsets[i-1] > wantOffsets[i] {
			t.Fatalf("entry data offsets are not sorted: %v", gotOffsets)
		}
	}
}
