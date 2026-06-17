package journal

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDirectoryReader(t *testing.T) {
	dir := createDirectoryReaderJournals(t, 3, 3)
	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()

	assertDirectoryReaderCount(t, dr, 9)
	assertDirectoryReaderUniqueIndex(t, dr, 3)
	assertDirectoryReaderPriority(t, dr)
}

func createDirectoryReaderJournals(t *testing.T, files int, entriesPerFile int) string {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "journal.d")
	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("MkdirAll error: %v", err)
	}
	for i := 0; i < files; i++ {
		path := filepath.Join(dir, "system@abc123-00000001-0000000"+string(rune('0'+i))+".journal")
		w, err := Create(path, testOptions())
		if err != nil {
			t.Fatalf("Create error: %v", err)
		}
		for j := 0; j < entriesPerFile; j++ {
			if err := w.Append([]Field{
				StringField("INDEX", string(rune('0'+i))),
				StringField("PRIORITY", "6"),
			}, EntryOptions{RealtimeUsec: uint64(1000 + i*10 + j), MonotonicUsec: uint64(i*entriesPerFile + j + 1)}); err != nil {
				t.Fatalf("Append error: %v", err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatalf("Close error: %v", err)
		}
	}
	return dir
}

func assertDirectoryReaderCount(t *testing.T, dr *DirectoryReader, want int) {
	t.Helper()
	count := 0
	for {
		ok, err := dr.Step()
		if err != nil {
			t.Fatalf("Step error: %v", err)
		}
		if !ok {
			break
		}
		count++
	}
	if count != want {
		t.Fatalf("DirectoryReader read %d entries, want %d", count, want)
	}
}

func assertDirectoryReaderUniqueIndex(t *testing.T, dr *DirectoryReader, want int) {
	t.Helper()
	values, err := dr.QueryUnique("INDEX")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique INDEX error: %v", err)
	}
	if len(values) != want {
		t.Fatalf("DirectoryReader QueryUnique INDEX returned %d values, want %d", len(values), want)
	}
}

func assertDirectoryReaderPriority(t *testing.T, dr *DirectoryReader) {
	t.Helper()
	values, err := dr.QueryUnique("PRIORITY")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY error: %v", err)
	}
	if len(values) != 1 || string(values[0]) != "6" {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY returned %#v, want one value 6", values)
	}
	var visited [][]byte
	if err := dr.VisitUnique("PRIORITY", func(value []byte) error {
		visited = append(visited, cloneBytes(value))
		return nil
	}); err != nil {
		t.Fatalf("DirectoryReader VisitUnique PRIORITY error: %v", err)
	}
	if len(visited) != 1 || string(visited[0]) != "6" {
		t.Fatalf("DirectoryReader VisitUnique PRIORITY returned %#v, want one value 6", visited)
	}
}

func TestDirectoryReaderSequentialFastPathOrdersNonOverlappingFiles(t *testing.T) {
	dir := t.TempDir()
	seqnumID := UUID{0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60}
	machineID := UUID{0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70}
	bootID := UUID{0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f, 0x80}
	first := filepath.Join(dir, "first.journal")
	second := filepath.Join(dir, "second.journal")

	for _, tc := range []struct {
		path     string
		head     uint64
		realtime uint64
		messages []string
	}{
		{first, 1, 1_700_006_000_000_000, []string{"first-a", "first-b"}},
		{second, 3, 1_700_006_000_000_010, []string{"second-a", "second-b"}},
	} {
		w, err := Create(tc.path, Options{
			MachineID:  machineID,
			BootID:     bootID,
			SeqnumID:   seqnumID,
			HeadSeqnum: tc.head,
		})
		if err != nil {
			t.Fatalf("Create(%s) error = %v", tc.path, err)
		}
		for i, message := range tc.messages {
			if err := w.Append([]Field{StringField("MESSAGE", message)}, EntryOptions{
				RealtimeUsec:  tc.realtime + uint64(i),
				MonotonicUsec: uint64(i + 1),
			}); err != nil {
				t.Fatalf("Append(%s) error = %v", message, err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatalf("Close(%s) error = %v", tc.path, err)
		}
	}

	reader, err := OpenFiles([]string{second, first})
	if err != nil {
		t.Fatalf("OpenFiles error = %v", err)
	}
	defer reader.Close()
	if !reader.nonOverlapping {
		t.Fatalf("nonOverlapping = false, want sequential fast path enabled")
	}

	gotForward := collectDirectoryMessages(t, reader, true)
	if want := "first-a,first-b,second-a,second-b"; strings.Join(gotForward, ",") != want {
		t.Fatalf("forward order = %q, want %q", gotForward, want)
	}
	if err := reader.SeekTail(); err != nil {
		t.Fatalf("SeekTail error = %v", err)
	}
	gotBackward := collectDirectoryMessages(t, reader, false)
	if want := "second-b,second-a,first-b,first-a"; strings.Join(gotBackward, ",") != want {
		t.Fatalf("backward order = %q, want %q", gotBackward, want)
	}
}

func collectDirectoryMessages(t *testing.T, reader *DirectoryReader, forward bool) []string {
	t.Helper()
	var got []string
	for {
		var (
			ok  bool
			err error
		)
		if forward {
			ok, err = reader.Step()
		} else {
			ok, err = reader.StepBack()
		}
		if err != nil {
			t.Fatalf("Step(forward=%v) error = %v", forward, err)
		}
		if !ok {
			return got
		}
		entry, err := reader.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry(forward=%v) error = %v", forward, err)
		}
		got = append(got, string(entry.Fields["MESSAGE"]))
	}
}

func TestDirectoryReaderSystemdZstdFixtures(t *testing.T) {
	dir := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc")

	r, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory systemd fixtures: %v", err)
	}
	defer r.Close()

	count := 0
	for {
		ok, err := r.Step()
		if err != nil {
			t.Fatalf("Step error: %v", err)
		}
		if !ok {
			break
		}
		count++
	}
	if count < 1000 {
		t.Fatalf("systemd fixture directory produced %d entries, want at least 1000", count)
	}

	boots, err := r.ListBoots()
	if err != nil {
		t.Fatalf("ListBoots error: %v", err)
	}
	if len(boots) != 4 {
		t.Fatalf("ListBoots returned %d boots, want 4", len(boots))
	}
}

func TestDirectoryReaderSystemdZstdFixturesBackward(t *testing.T) {
	dir := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc")

	r, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory systemd fixtures: %v", err)
	}
	defer r.Close()

	if err := r.SeekTail(); err != nil {
		t.Fatalf("SeekTail error: %v", err)
	}

	count := 0
	for {
		ok, err := r.StepBack()
		if err != nil {
			t.Fatalf("StepBack error: %v", err)
		}
		if !ok {
			break
		}
		entry, err := r.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry error: %v", err)
		}
		if len(entry.Fields) == 0 {
			t.Fatalf("backward entry %d has no fields", count)
		}
		count++
		if count >= 100 {
			break
		}
	}
	if count == 0 {
		t.Fatal("backward directory read produced no entries")
	}
}
