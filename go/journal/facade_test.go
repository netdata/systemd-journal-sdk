package journal

import (
	"path/filepath"
	"testing"
)

func TestSdJournalSeekCursorMatchesFullCursor(t *testing.T) {
	path := filepath.Join(t.TempDir(), "cursor.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for _, msg := range []string{"first", "second"} {
		if err := w.Append([]Field{StringField("MESSAGE", msg)}, EntryOptions{RealtimeUsec: 1_700_000_000}); err != nil {
			t.Fatalf("Append %q error: %v", msg, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpen(path, 0)
	if err != nil {
		t.Fatalf("SdJournalOpen error: %v", err)
	}
	defer j.Close()

	if n, err := j.Next(); err != nil || n != 1 {
		t.Fatalf("first Next = %d, %v", n, err)
	}
	first, err := j.GetEntry()
	if err != nil {
		t.Fatalf("first GetEntry error: %v", err)
	}
	if n, err := j.Next(); err != nil || n != 1 {
		t.Fatalf("second Next = %d, %v", n, err)
	}
	second, err := j.GetEntry()
	if err != nil {
		t.Fatalf("second GetEntry error: %v", err)
	}

	if err := SdJournalSeekCursor(j, second.Cursor); err != nil {
		t.Fatalf("SdJournalSeekCursor(second) error: %v", err)
	}
	got, err := j.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry after second cursor error: %v", err)
	}
	if string(got.Fields["MESSAGE"]) != "second" {
		t.Fatalf("cursor seek landed on %q, want second", got.Fields["MESSAGE"])
	}

	if err := SdJournalSeekCursor(j, first.Cursor); err != nil {
		t.Fatalf("SdJournalSeekCursor(first) error: %v", err)
	}
	got, err = j.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry after first cursor error: %v", err)
	}
	if string(got.Fields["MESSAGE"]) != "first" {
		t.Fatalf("cursor seek landed on %q, want first", got.Fields["MESSAGE"])
	}
}

func TestSdJournalGetEntryWithRealtimeRestoresPosition(t *testing.T) {
	path := filepath.Join(t.TempDir(), "realtime.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for i, msg := range []string{"first", "second", "third"} {
		if err := w.Append([]Field{StringField("MESSAGE", msg)}, EntryOptions{RealtimeUsec: uint64(1000 + i)}); err != nil {
			t.Fatalf("Append %q error: %v", msg, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpen(path, 0)
	if err != nil {
		t.Fatalf("SdJournalOpen error: %v", err)
	}
	defer j.Close()

	if n, err := j.Next(); err != nil || n != 1 {
		t.Fatalf("Next = %d, %v", n, err)
	}
	before, err := j.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry before realtime lookup: %v", err)
	}

	found, err := SdJournalGetEntryWithRealtime(j, 1001)
	if err != nil {
		t.Fatalf("SdJournalGetEntryWithRealtime error: %v", err)
	}
	if string(found.Fields["MESSAGE"]) != "second" {
		t.Fatalf("realtime lookup returned %q, want second", found.Fields["MESSAGE"])
	}

	after, err := j.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry after realtime lookup: %v", err)
	}
	if after.Cursor != before.Cursor {
		t.Fatalf("realtime lookup changed cursor from %q to %q", before.Cursor, after.Cursor)
	}
}

func TestSdJournalAddMatchValidatesWithoutJournalctlSyntax(t *testing.T) {
	path := filepath.Join(t.TempDir(), "match.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for _, msg := range []string{"alpha", "beta", "gamma"} {
		if err := w.Append([]Field{StringField("MESSAGE", msg)}, EntryOptions{}); err != nil {
			t.Fatalf("Append %q error: %v", msg, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpen(path, 0)
	if err != nil {
		t.Fatalf("SdJournalOpen error: %v", err)
	}
	defer j.Close()

	if err := SdJournalAddMatch(j, []byte("MESSAGE=alpha")); err != nil {
		t.Fatalf("SdJournalAddMatch alpha error: %v", err)
	}
	if err := SdJournalAddDisjunction(j); err != nil {
		t.Fatalf("SdJournalAddDisjunction error: %v", err)
	}
	if err := SdJournalAddMatch(j, []byte("MESSAGE=beta")); err != nil {
		t.Fatalf("SdJournalAddMatch beta error: %v", err)
	}
	if err := SdJournalAddMatch(j, []byte("message=alpha")); err == nil {
		t.Fatal("SdJournalAddMatch accepted invalid lowercase field")
	}

	count := 0
	for {
		n, err := j.Next()
		if err != nil {
			t.Fatalf("Next error: %v", err)
		}
		if n == 0 {
			break
		}
		count++
	}
	if count != 2 {
		t.Fatalf("facade disjunction matched %d entries, want 2", count)
	}
}
