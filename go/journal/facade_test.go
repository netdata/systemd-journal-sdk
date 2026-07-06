package journal

import (
	"bytes"
	"errors"
	"path/filepath"
	"reflect"
	"testing"
)

func TestSdJournalSeekCursorMatchesFullCursor(t *testing.T) {
	path := filepath.Join(t.TempDir(), "cursor.journal")
	createMessageJournal(t, path, []messageRow{
		{message: "first", realtime: 1_700_000_000},
		{message: "second", realtime: 1_700_000_000},
	})
	j := openSdJournal(t, path)
	defer j.Close()

	requireNext(t, j, "first")
	first := requireEntry(t, j, "first")
	requireNext(t, j, "second")
	second := requireEntry(t, j, "second")

	assertCursorSeeksToMessage(t, j, second.Cursor, "second")
	assertCursorSeeksToMessage(t, j, first.Cursor, "first")
}

type messageRow struct {
	message  string
	realtime uint64
}

func createMessageJournal(t *testing.T, path string, rows []messageRow) {
	t.Helper()
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	for i, row := range rows {
		if err := w.Append([]Field{StringField("MESSAGE", row.message)}, EntryOptions{RealtimeUsec: row.realtime, MonotonicUsec: uint64(i + 1)}); err != nil {
			t.Fatalf("Append %q error: %v", row.message, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}
}

func openSdJournal(t *testing.T, path string) *sdJournal {
	t.Helper()
	j, err := SdJournalOpen(path, 0)
	if err != nil {
		t.Fatalf("SdJournalOpen error: %v", err)
	}
	return j
}

func requireNext(t *testing.T, j *sdJournal, label string) {
	t.Helper()
	if n, err := j.Next(); err != nil || n != 1 {
		t.Fatalf("%s Next = %d, %v", label, n, err)
	}
}

func requireEntry(t *testing.T, j *sdJournal, label string) *Entry {
	t.Helper()
	entry, err := j.GetEntry()
	if err != nil {
		t.Fatalf("%s GetEntry error: %v", label, err)
	}
	return entry
}

func assertCursorSeeksToMessage(t *testing.T, j *sdJournal, cursor string, want string) {
	t.Helper()
	if err := SdJournalSeekCursor(j, cursor); err != nil {
		t.Fatalf("SdJournalSeekCursor(%s) error: %v", want, err)
	}
	got := requireEntry(t, j, "after "+want+" cursor")
	if string(got.Fields["MESSAGE"]) != want {
		t.Fatalf("cursor seek landed on %q, want %s", got.Fields["MESSAGE"], want)
	}
}

func TestSdJournalGetEntryWithRealtimeRestoresPosition(t *testing.T) {
	path := filepath.Join(t.TempDir(), "realtime.journal")
	createMessageJournal(t, path, []messageRow{{"first", 1000}, {"second", 1001}, {"third", 1002}})
	j := openSdJournal(t, path)
	defer j.Close()

	requireNext(t, j, "before realtime lookup")
	before := requireEntry(t, j, "before realtime lookup")

	found, err := SdJournalGetEntryWithRealtime(j, 1001)
	if err != nil {
		t.Fatalf("SdJournalGetEntryWithRealtime error: %v", err)
	}
	if string(found.Fields["MESSAGE"]) != "second" {
		t.Fatalf("realtime lookup returned %q, want second", found.Fields["MESSAGE"])
	}

	after := requireEntry(t, j, "after realtime lookup")
	if after.Cursor != before.Cursor {
		t.Fatalf("realtime lookup changed cursor from %q to %q", before.Cursor, after.Cursor)
	}
}

func TestSdJournalAddMatchValidatesWithoutJournalctlSyntax(t *testing.T) {
	path := filepath.Join(t.TempDir(), "match.journal")
	createMessageJournal(t, path, []messageRow{{"alpha", 0}, {"beta", 0}, {"gamma", 0}})
	j := openSdJournal(t, path)
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

	if count := countFacadeRows(t, j); count != 2 {
		t.Fatalf("facade disjunction matched %d entries, want 2", count)
	}
}

func countFacadeRows(t *testing.T, j *sdJournal) int {
	t.Helper()
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
	return count
}

func TestSdJournalQueryUniqueBinaryValues(t *testing.T) {
	path := filepath.Join(t.TempDir(), "unique-binary.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{{Name: "BINARY", Value: []byte{0x00, 0xff}}}, testEntryOptions(1)); err != nil {
		t.Fatalf("Append binary error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpen(path, 0)
	if err != nil {
		t.Fatalf("SdJournalOpen error: %v", err)
	}
	defer j.Close()

	values, err := SdJournalQueryUnique(j, "BINARY")
	if err != nil {
		t.Fatalf("SdJournalQueryUnique error: %v", err)
	}
	if len(values) != 1 {
		t.Fatalf("SdJournalQueryUnique values = %#v", values)
	}
	if values[0].Field != "BINARY" || !bytes.Equal(values[0].Value, []byte{0x00, 0xff}) {
		t.Fatalf("SdJournalQueryUnique binary value = %#v", values)
	}
}

func TestSdJournalDataPayloadsRemainValidForCurrentRow(t *testing.T) {
	path := filepath.Join(t.TempDir(), "facade-row-lifetime.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "first"),
		{Name: "REPEAT", Value: []byte("one")},
		{Name: "REPEAT", Value: []byte("two")},
	}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 11}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpenFiles([]string{path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles error: %v", err)
	}
	defer j.Close()

	if n, err := SdJournalNext(j); err != nil || n != 1 {
		t.Fatalf("Next = %d, %v", n, err)
	}
	if err := SdJournalRestartData(j); err != nil {
		t.Fatalf("SdJournalRestartData error: %v", err)
	}

	payloads := collectFacadeData(t, j, SdJournalEnumerateAvailableData)
	for _, want := range [][]byte{
		[]byte("MESSAGE=first"),
		[]byte("REPEAT=one"),
		[]byte("REPEAT=two"),
	} {
		if !containsPayload(payloads, want) {
			t.Fatalf("cached row payloads after end-of-row = %q, missing %q", payloads, want)
		}
	}
}

func TestSdJournalCompressedMixedDataPayloadsRemainValidForCurrentRow(t *testing.T) {
	path := filepath.Join(t.TempDir(), "facade-compressed-row-lifetime.journal")
	opts := testOptions()
	opts.Compression = CompressionZSTD
	opts.CompressThresholdBytes = 8
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	largeValue := bytes.Repeat([]byte("mixed "), 256)
	if err := w.Append([]Field{
		{Name: "SMALL", Value: []byte("x")},
		{Name: "LARGE", Value: largeValue},
	}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 11}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpenFiles([]string{path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles error: %v", err)
	}
	defer j.Close()

	if n, err := SdJournalNext(j); err != nil || n != 1 {
		t.Fatalf("Next = %d, %v", n, err)
	}
	if err := SdJournalRestartData(j); err != nil {
		t.Fatalf("SdJournalRestartData error: %v", err)
	}

	payloads := collectFacadeData(t, j, SdJournalEnumerateAvailableData)
	wantLarge := append([]byte("LARGE="), largeValue...)
	for _, want := range [][]byte{
		[]byte("SMALL=x"),
		wantLarge,
	} {
		if !containsPayload(payloads, want) {
			t.Fatalf("cached compressed/mixed row payloads after end-of-row missing %q", want)
		}
	}
}

func TestSdJournalStatefulUniqueHandlesCompressedPayloadsAndRestart(t *testing.T) {
	path := filepath.Join(t.TempDir(), "facade-compressed-unique.journal")
	opts := testOptions()
	opts.Compression = CompressionZSTD
	opts.CompressThresholdBytes = 8
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	largeValue := bytes.Repeat([]byte("unique "), 256)
	if err := w.Append([]Field{
		{Name: "MESSAGE", Value: largeValue},
	}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 11}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := SdJournalOpenFiles([]string{path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles error: %v", err)
	}
	defer j.Close()

	if err := SdJournalQueryUniqueState(j, "MESSAGE"); err != nil {
		t.Fatalf("SdJournalQueryUniqueState error: %v", err)
	}
	want := append([]byte("MESSAGE="), largeValue...)
	payload, ok, err := SdJournalEnumerateAvailableUnique(j)
	if err != nil {
		t.Fatalf("SdJournalEnumerateAvailableUnique error: %v", err)
	}
	if !ok || !bytes.Equal(payload, want) {
		t.Fatalf("compressed unique payload = %q, %v; want %q true", payload, ok, want)
	}
	if payload, ok, err = SdJournalEnumerateAvailableUnique(j); err != nil || ok || payload != nil {
		t.Fatalf("compressed unique end = %q, %v, %v; want nil false nil", payload, ok, err)
	}
	if err := SdJournalRestartUnique(j); err != nil {
		t.Fatalf("SdJournalRestartUnique error: %v", err)
	}
	payload, ok, err = SdJournalEnumerateAvailableUnique(j)
	if err != nil {
		t.Fatalf("restarted SdJournalEnumerateAvailableUnique error: %v", err)
	}
	if !ok || !bytes.Equal(payload, want) {
		t.Fatalf("restarted compressed unique payload = %q, %v; want %q true", payload, ok, want)
	}
}

func TestSdJournalJfFacadeStatefulReaderOperations(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "jf-facade.journal")
	createJfFacadeJournal(t, path)

	j, err := SdJournalOpenFiles([]string{path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles error: %v", err)
	}
	defer j.Close()

	requireSdJournalNext(t, j, "first facade row")
	assertJfFacadeFirstMetadata(t, j)
	assertJfFacadeFirstData(t, j)
	assertJfFacadeUniqueAndFields(t, j)
	assertJfFacadeRealtimeAndCursor(t, j)
	assertJfFacadeMultiFile(t, dir, path)
}

func createJfFacadeJournal(t *testing.T, path string) {
	t.Helper()
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "first"),
		{Name: "REPEAT", Value: []byte("one")},
		{Name: "REPEAT", Value: []byte("two")},
		{Name: "BIN", Value: []byte{0x00, 0xff}},
	}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 11}); err != nil {
		t.Fatalf("Append first error: %v", err)
	}
	if err := w.Append([]Field{
		StringField("MESSAGE", "second"),
		{Name: "REPEAT", Value: []byte("three")},
	}, EntryOptions{RealtimeUsec: 1001, MonotonicUsec: 12}); err != nil {
		t.Fatalf("Append second error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}
}

func assertJfFacadeFirstMetadata(t *testing.T, j *sdJournal) {
	t.Helper()
	seqnum, seqnumID, err := SdJournalGetSeqnum(j)
	if err != nil {
		t.Fatalf("SdJournalGetSeqnum error: %v", err)
	}
	if seqnum != 1 || isZeroUUID(seqnumID) {
		t.Fatalf("seqnum metadata = %d %s", seqnum, seqnumID)
	}
	monotonic, bootID, err := SdJournalGetMonotonicUsec(j)
	if err != nil {
		t.Fatalf("SdJournalGetMonotonicUsec error: %v", err)
	}
	if monotonic != 11 || isZeroUUID(bootID) {
		t.Fatalf("monotonic metadata = %d %s", monotonic, bootID)
	}
}

func assertJfFacadeFirstData(t *testing.T, j *sdJournal) {
	t.Helper()
	if err := SdJournalRestartData(j); err != nil {
		t.Fatalf("SdJournalRestartData error: %v", err)
	}
	payloads := collectFacadeData(t, j, SdJournalEnumerateAvailableData)
	requirePayloads(t, "data enumeration", payloads, [][]byte{
		[]byte("REPEAT=one"),
		[]byte("REPEAT=two"),
		[]byte("BIN=\x00\xff"),
	})
	data, err := SdJournalGetData(j, "REPEAT")
	if err != nil {
		t.Fatalf("SdJournalGetData error: %v", err)
	}
	if !bytes.Equal(data, []byte("REPEAT=one")) {
		t.Fatalf("GetData(REPEAT) = %q", data)
	}
}

func assertJfFacadeUniqueAndFields(t *testing.T, j *sdJournal) {
	t.Helper()
	if err := SdJournalQueryUniqueState(j, "REPEAT"); err != nil {
		t.Fatalf("SdJournalQueryUniqueState error: %v", err)
	}
	unique := collectFacadeData(t, j, SdJournalEnumerateAvailableUnique)
	requirePayloads(t, "unique enumeration", unique, [][]byte{
		[]byte("REPEAT=one"),
		[]byte("REPEAT=two"),
		[]byte("REPEAT=three"),
	})

	if err := SdJournalRestartFields(j); err != nil {
		t.Fatalf("SdJournalRestartFields error: %v", err)
	}
	fields := collectFacadeFields(t, j)
	for _, field := range []string{"MESSAGE", "REPEAT", "BIN"} {
		if !fields[field] {
			t.Fatalf("field %s missing from stateful field enumeration: %v", field, fields)
		}
	}
}

func assertJfFacadeRealtimeAndCursor(t *testing.T, j *sdJournal) {
	t.Helper()
	if err := SdJournalSeekRealtimeUsec(j, 1001); err != nil {
		t.Fatalf("SdJournalSeekRealtimeUsec forward error: %v", err)
	}
	requireSdJournalNext(t, j, "after seek realtime")
	requireSdJournalMessage(t, j, "seek realtime forward", "second")

	if err := SdJournalSeekRealtimeUsec(j, 1001); err != nil {
		t.Fatalf("SdJournalSeekRealtimeUsec backward error: %v", err)
	}
	if n, err := SdJournalPrevious(j); err != nil || n != 1 {
		t.Fatalf("Previous after seek realtime = %d, %v", n, err)
	}
	requireSdJournalMessage(t, j, "seek realtime backward", "second")

	cursor, err := SdJournalGetCursor(j)
	if err != nil {
		t.Fatalf("SdJournalGetCursor error: %v", err)
	}
	if ok, err := SdJournalTestCursor(j, cursor); err != nil || !ok {
		t.Fatalf("SdJournalTestCursor(current) = %v, %v", ok, err)
	}
	if ok, err := SdJournalTestCursor(j, "invalid-cursor"); err != nil || ok {
		t.Fatalf("SdJournalTestCursor(invalid) = %v, %v", ok, err)
	}
}

func assertJfFacadeMultiFile(t *testing.T, dir string, path string) {
	t.Helper()
	path2 := filepath.Join(dir, "jf-facade-second.journal")
	w2, err := Create(path2, testOptions())
	if err != nil {
		t.Fatalf("Create second error: %v", err)
	}
	if err := w2.Append([]Field{
		StringField("MESSAGE", "third"),
		{Name: "REPEAT", Value: []byte("four")},
	}, EntryOptions{RealtimeUsec: 1002, MonotonicUsec: 21}); err != nil {
		t.Fatalf("Append third error: %v", err)
	}
	if err := w2.Close(); err != nil {
		t.Fatalf("Close second error: %v", err)
	}

	multi, err := SdJournalOpenFiles([]string{path2, path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles multi error: %v", err)
	}
	defer multi.Close()

	// systemd compares same-source seqnums before realtime when interleaving files.
	wantMessages := []string{"first", "third", "second"}
	if messages := collectSdJournalMessages(t, multi); !reflect.DeepEqual(messages, wantMessages) {
		t.Fatalf("multi messages = %v, want %v", messages, wantMessages)
	}

	if err := SdJournalSeekRealtimeUsec(multi, 1002); err != nil {
		t.Fatalf("multi seek realtime backward error: %v", err)
	}
	if n, err := SdJournalPrevious(multi); err != nil || n != 1 {
		t.Fatalf("multi previous after seek = %d, %v", n, err)
	}
	requireSdJournalMessage(t, multi, "multi backward seek", "second")

	if err := SdJournalSeekRealtimeUsec(multi, 999); err != nil {
		t.Fatalf("multi seek before range error: %v", err)
	}
	if n, err := SdJournalPrevious(multi); err != nil || n != 0 {
		t.Fatalf("multi previous before range = %d, %v", n, err)
	}
}

func requireSdJournalNext(t *testing.T, j *sdJournal, context string) {
	t.Helper()
	if n, err := SdJournalNext(j); err != nil || n != 1 {
		t.Fatalf("Next %s = %d, %v", context, n, err)
	}
}

func requireSdJournalMessage(t *testing.T, j *sdJournal, context string, want string) {
	t.Helper()
	entry, err := SdJournalGetEntry(j)
	if err != nil {
		t.Fatalf("GetEntry %s error: %v", context, err)
	}
	if got := string(entry.Fields["MESSAGE"]); got != want {
		t.Fatalf("%s landed on %q, want %q", context, got, want)
	}
}

func requirePayloads(t *testing.T, context string, payloads [][]byte, want [][]byte) {
	t.Helper()
	for _, payload := range want {
		if !containsPayload(payloads, payload) {
			t.Fatalf("%s missing %q in %q", context, payload, payloads)
		}
	}
}

func collectFacadeFields(t *testing.T, j *sdJournal) map[string]bool {
	t.Helper()
	fields := map[string]bool{}
	for {
		field, ok, err := SdJournalEnumerateField(j)
		if err != nil {
			t.Fatalf("SdJournalEnumerateField error: %v", err)
		}
		if !ok {
			return fields
		}
		fields[field] = true
	}
}

func collectSdJournalMessages(t *testing.T, j *sdJournal) []string {
	t.Helper()
	var messages []string
	for {
		n, err := SdJournalNext(j)
		if err != nil {
			t.Fatalf("multi next error: %v", err)
		}
		if n == 0 {
			return messages
		}
		entry, err := SdJournalGetEntry(j)
		if err != nil {
			t.Fatalf("multi get entry error: %v", err)
		}
		messages = append(messages, string(entry.Fields["MESSAGE"]))
	}
}

func TestDirectoryReaderPreviousBeforeRealtimeRange(t *testing.T) {
	dir := t.TempDir()
	first := filepath.Join(dir, "first.journal")
	second := filepath.Join(dir, "second.journal")

	for _, tc := range []struct {
		path     string
		message  string
		realtime uint64
	}{
		{first, "first", 1000},
		{second, "second", 2000},
	} {
		w, err := Create(tc.path, testOptions())
		if err != nil {
			t.Fatalf("Create %s error: %v", tc.message, err)
		}
		if err := w.Append([]Field{StringField("MESSAGE", tc.message)}, EntryOptions{RealtimeUsec: tc.realtime, MonotonicUsec: tc.realtime}); err != nil {
			t.Fatalf("Append %s error: %v", tc.message, err)
		}
		if err := w.Close(); err != nil {
			t.Fatalf("Close %s error: %v", tc.message, err)
		}
	}

	reader, err := OpenFiles([]string{second, first})
	if err != nil {
		t.Fatalf("OpenFiles error: %v", err)
	}
	defer reader.Close()

	if err := reader.SeekRealtimeUsec(999); err != nil {
		t.Fatalf("SeekRealtimeUsec before range error: %v", err)
	}
	if err := reader.Previous(); !errors.Is(err, errStartOfEntries) {
		t.Fatalf("Previous before range = %v, want %v", err, errStartOfEntries)
	}
}

func collectFacadeData(t *testing.T, j *sdJournal, next func(*sdJournal) ([]byte, bool, error)) [][]byte {
	t.Helper()
	var out [][]byte
	for {
		payload, ok, err := next(j)
		if err != nil {
			t.Fatalf("enumerate data error: %v", err)
		}
		if !ok {
			return out
		}
		out = append(out, payload)
	}
}

func containsPayload(payloads [][]byte, want []byte) bool {
	for _, payload := range payloads {
		if bytes.Equal(payload, want) {
			return true
		}
	}
	return false
}
