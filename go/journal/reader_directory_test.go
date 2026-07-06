package journal

import (
	"fmt"
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

func createHighCardinalityDirectoryReaderJournal(tb testing.TB, files int, entriesPerFile int) string {
	tb.Helper()
	dir := filepath.Join(tb.TempDir(), "journal.d")
	if err := os.MkdirAll(dir, 0o750); err != nil {
		tb.Fatalf("MkdirAll error: %v", err)
	}
	for i := 0; i < files; i++ {
		path := filepath.Join(dir, "high-cardinality-"+string(rune('a'+i))+".journal")
		w, err := Create(path, testOptions())
		if err != nil {
			tb.Fatalf("Create error: %v", err)
		}
		for j := 0; j < entriesPerFile; j++ {
			value := []byte(fmt.Sprintf("value-%04d", j))
			if err := w.Append([]Field{
				{Name: "UNIQUE_ID", Value: value},
				StringField("PRIORITY", "6"),
			}, EntryOptions{RealtimeUsec: uint64(1_700_000_000_000_000 + i*entriesPerFile + j), MonotonicUsec: uint64(i*entriesPerFile + j + 1)}); err != nil {
				tb.Fatalf("Append error: %v", err)
			}
		}
		if err := w.Close(); err != nil {
			tb.Fatalf("Close error: %v", err)
		}
	}
	return dir
}

func BenchmarkDirectoryReaderUniqueHighCardinalityColdBuild(b *testing.B) {
	dir := createHighCardinalityDirectoryReaderJournal(b, 3, 500)
	dr, err := OpenDirectory(dir)
	if err != nil {
		b.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()

	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		dr.uniqueCache = make(map[directoryUniqueCacheKey][][]byte)
		dr.uniqueCacheOrder = nil
		if err := dr.QueryUniqueState("UNIQUE_ID"); err != nil {
			b.Fatalf("QueryUniqueState error: %v", err)
		}
		count := 0
		for {
			_, ok, err := dr.EnumerateUniquePayload()
			if err != nil {
				b.Fatalf("EnumerateUniquePayload error: %v", err)
			}
			if !ok {
				break
			}
			count++
		}
		if count != 500 {
			b.Fatalf("enumerated %d unique values, want 500", count)
		}
	}
}

func BenchmarkDirectoryReaderUniqueHighCardinalityCachedRestart(b *testing.B) {
	dir := createHighCardinalityDirectoryReaderJournal(b, 3, 500)
	dr, err := OpenDirectory(dir)
	if err != nil {
		b.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()
	if err := dr.QueryUniqueState("UNIQUE_ID"); err != nil {
		b.Fatalf("QueryUniqueState error: %v", err)
	}

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := dr.RestartUniqueState(); err != nil {
			b.Fatalf("RestartUniqueState error: %v", err)
		}
		count := 0
		for {
			_, ok, err := dr.EnumerateUniquePayload()
			if err != nil {
				b.Fatalf("EnumerateUniquePayload error: %v", err)
			}
			if !ok {
				break
			}
			count++
		}
		if count != 500 {
			b.Fatalf("enumerated %d unique values, want 500", count)
		}
	}
}

func TestDirectoryReaderUniqueStateSurvivesCachePressure(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "journal.d")
	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("MkdirAll error: %v", err)
	}
	path := filepath.Join(dir, "cache-pressure.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	firstFields := []Field{StringField("ACTIVE", "one")}
	for i := 0; i < 10; i++ {
		firstFields = append(firstFields, StringField(fmt.Sprintf("CACHE_%02d", i), fmt.Sprintf("value-%02d", i)))
	}
	if err := w.Append(firstFields, EntryOptions{RealtimeUsec: 10_000, MonotonicUsec: 100}); err != nil {
		t.Fatalf("Append first entry error: %v", err)
	}
	if err := w.Append([]Field{StringField("ACTIVE", "two")}, EntryOptions{RealtimeUsec: 10_001, MonotonicUsec: 101}); err != nil {
		t.Fatalf("Append second entry error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()
	if err := dr.QueryUniqueState("ACTIVE"); err != nil {
		t.Fatalf("QueryUniqueState ACTIVE error: %v", err)
	}
	payload, ok, err := dr.EnumerateUniquePayload()
	if err != nil {
		t.Fatalf("first EnumerateUniquePayload error: %v", err)
	}
	if !ok {
		t.Fatal("first EnumerateUniquePayload returned no payload")
	}
	got := map[string]struct{}{string(payload): {}}

	for i := 0; i < 10; i++ {
		field := fmt.Sprintf("CACHE_%02d", i)
		values, err := dr.QueryUnique(field)
		if err != nil {
			t.Fatalf("QueryUnique %s error: %v", field, err)
		}
		want := fmt.Sprintf("value-%02d", i)
		if len(values) != 1 || string(values[0]) != want {
			t.Fatalf("QueryUnique %s = %#v, want %q", field, values, want)
		}
	}

	for {
		payload, ok, err := dr.EnumerateUniquePayload()
		if err != nil {
			t.Fatalf("EnumerateUniquePayload after cache pressure error: %v", err)
		}
		if !ok {
			break
		}
		got[string(payload)] = struct{}{}
	}
	if _, ok := got["ACTIVE=one"]; !ok {
		t.Fatalf("ACTIVE=one missing after cache pressure: %#v", got)
	}
	if _, ok := got["ACTIVE=two"]; !ok {
		t.Fatalf("ACTIVE=two missing after cache pressure: %#v", got)
	}
	if len(got) != 2 {
		t.Fatalf("ACTIVE unique payloads after cache pressure = %#v, want exactly two", got)
	}
}

func TestDirectoryReaderUniqueCacheInvalidatesAfterLiveAppend(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "journal.d")
	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("MkdirAll error: %v", err)
	}
	path := filepath.Join(dir, "live-unique.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{StringField("HOST", "host-a")}, EntryOptions{RealtimeUsec: 10_000, MonotonicUsec: 100}); err != nil {
		t.Fatalf("Append initial entry error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close initial writer error: %v", err)
	}

	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()
	values, err := dr.QueryUnique("HOST")
	if err != nil {
		t.Fatalf("initial QueryUnique error: %v", err)
	}
	if len(values) != 1 || string(values[0]) != "host-a" {
		t.Fatalf("initial QueryUnique = %#v, want host-a", values)
	}
	if dr.uniqueCacheBuilds != 1 {
		t.Fatalf("initial uniqueCacheBuilds = %d, want 1", dr.uniqueCacheBuilds)
	}

	appendWriter, err := Open(path)
	if err != nil {
		t.Fatalf("Open append writer error: %v", err)
	}
	if err := appendWriter.Append([]Field{StringField("HOST", "host-b")}, EntryOptions{RealtimeUsec: 10_001, MonotonicUsec: 101}); err != nil {
		t.Fatalf("Append live entry error: %v", err)
	}
	if err := appendWriter.Close(); err != nil {
		t.Fatalf("Close append writer error: %v", err)
	}

	values, err = dr.QueryUnique("HOST")
	if err != nil {
		t.Fatalf("post-append QueryUnique error: %v", err)
	}
	got := map[string]struct{}{}
	for _, value := range values {
		got[string(value)] = struct{}{}
	}
	if _, ok := got["host-a"]; !ok {
		t.Fatalf("post-append QueryUnique missing host-a: %#v", got)
	}
	if _, ok := got["host-b"]; !ok {
		t.Fatalf("post-append QueryUnique missing host-b: %#v", got)
	}
	if len(got) != 2 {
		t.Fatalf("post-append QueryUnique = %#v, want exactly host-a and host-b", got)
	}
	if dr.uniqueCacheBuilds != 2 {
		t.Fatalf("post-append uniqueCacheBuilds = %d, want 2", dr.uniqueCacheBuilds)
	}
}

func TestReaderUniqueRefreshPreservesLiveEntryIteration(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "live-reader.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{
		StringField("HOST", "host-a"),
		StringField("MESSAGE", "first"),
	}, EntryOptions{RealtimeUsec: 10_000, MonotonicUsec: 100}); err != nil {
		t.Fatalf("Append initial entry error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close initial writer error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	appendWriter, err := Open(path)
	if err != nil {
		t.Fatalf("Open append writer error: %v", err)
	}
	if err := appendWriter.Append([]Field{
		StringField("HOST", "host-b"),
		StringField("MESSAGE", "second"),
	}, EntryOptions{RealtimeUsec: 10_001, MonotonicUsec: 101}); err != nil {
		t.Fatalf("Append live entry error: %v", err)
	}
	if err := appendWriter.Close(); err != nil {
		t.Fatalf("Close append writer error: %v", err)
	}

	if err := r.QueryUniqueState("HOST"); err != nil {
		t.Fatalf("QueryUniqueState HOST error: %v", err)
	}
	var hosts []string
	for {
		payload, ok, err := r.EnumerateUniquePayload()
		if err != nil {
			t.Fatalf("EnumerateUniquePayload error: %v", err)
		}
		if !ok {
			break
		}
		hosts = append(hosts, string(payload))
	}
	hostSet := map[string]struct{}{}
	for _, host := range hosts {
		hostSet[host] = struct{}{}
	}
	if _, ok := hostSet["HOST=host-a"]; !ok {
		t.Fatalf("unique HOST missing host-a: %#v", hosts)
	}
	if _, ok := hostSet["HOST=host-b"]; !ok {
		t.Fatalf("unique HOST missing host-b: %#v", hosts)
	}

	r.SeekHead()
	var messages []string
	for {
		ok, err := r.Step()
		if err != nil {
			t.Fatalf("Step error: %v", err)
		}
		if !ok {
			break
		}
		entry, err := r.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry error: %v", err)
		}
		messages = append(messages, string(entry.Fields["MESSAGE"]))
	}
	if strings.Join(messages, ",") != "first,second" {
		t.Fatalf("messages after unique refresh = %#v, want first,second", messages)
	}
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
	buildsBefore := dr.uniqueCacheBuilds
	values, err := dr.QueryUnique("INDEX")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique INDEX error: %v", err)
	}
	if dr.uniqueCacheBuilds != buildsBefore+1 {
		t.Fatalf("DirectoryReader QueryUnique INDEX cache builds = %d, want %d", dr.uniqueCacheBuilds, buildsBefore+1)
	}
	if len(values) != want {
		t.Fatalf("DirectoryReader QueryUnique INDEX returned %d values, want %d", len(values), want)
	}
}

func assertDirectoryReaderPriority(t *testing.T, dr *DirectoryReader) {
	t.Helper()
	buildsBefore := dr.uniqueCacheBuilds
	values, err := dr.QueryUnique("PRIORITY")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY error: %v", err)
	}
	if dr.uniqueCacheBuilds != buildsBefore+1 {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY cache builds = %d, want %d", dr.uniqueCacheBuilds, buildsBefore+1)
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
	if dr.uniqueCacheBuilds != buildsBefore+1 {
		t.Fatalf("DirectoryReader VisitUnique PRIORITY rebuilt cache: builds = %d, want %d", dr.uniqueCacheBuilds, buildsBefore+1)
	}
	if len(visited) != 1 || string(visited[0]) != "6" {
		t.Fatalf("DirectoryReader VisitUnique PRIORITY returned %#v, want one value 6", visited)
	}
	if err := dr.QueryUniqueState("PRIORITY"); err != nil {
		t.Fatalf("DirectoryReader QueryUniqueState PRIORITY error: %v", err)
	}
	if dr.uniqueCacheBuilds != buildsBefore+1 {
		t.Fatalf("DirectoryReader QueryUniqueState PRIORITY rebuilt cache: builds = %d, want %d", dr.uniqueCacheBuilds, buildsBefore+1)
	}
	payload, ok, err := dr.EnumerateUniquePayload()
	if err != nil {
		t.Fatalf("DirectoryReader EnumerateUniquePayload error: %v", err)
	}
	if !ok || string(payload) != "PRIORITY=6" {
		t.Fatalf("DirectoryReader EnumerateUniquePayload = %q, %v; want PRIORITY=6 true", payload, ok)
	}
	if err := dr.RestartUniqueState(); err != nil {
		t.Fatalf("DirectoryReader RestartUniqueState error: %v", err)
	}
	if dr.uniqueCacheBuilds != buildsBefore+1 {
		t.Fatalf("DirectoryReader RestartUniqueState rebuilt cache: builds = %d, want %d", dr.uniqueCacheBuilds, buildsBefore+1)
	}
	payload, ok, err = dr.EnumerateUniquePayload()
	if err != nil {
		t.Fatalf("DirectoryReader restarted EnumerateUniquePayload error: %v", err)
	}
	if !ok || string(payload) != "PRIORITY=6" {
		t.Fatalf("DirectoryReader restarted EnumerateUniquePayload = %q, %v; want PRIORITY=6 true", payload, ok)
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
