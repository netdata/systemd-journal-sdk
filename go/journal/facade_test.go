package journal

import (
	"bytes"
	"errors"
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

func TestSdJournalQueryUniqueBinaryValues(t *testing.T) {
	path := filepath.Join(t.TempDir(), "unique-binary.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Append([]Field{{Name: "BINARY", Value: []byte{0x00, 0xff}}}, EntryOptions{}); err != nil {
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

func TestSdJournalJfFacadeStatefulReaderOperations(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "jf-facade.journal")
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

	j, err := SdJournalOpenFiles([]string{path}, 0)
	if err != nil {
		t.Fatalf("SdJournalOpenFiles error: %v", err)
	}
	defer j.Close()

	if n, err := SdJournalNext(j); err != nil || n != 1 {
		t.Fatalf("Next = %d, %v", n, err)
	}
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

	if err := SdJournalRestartData(j); err != nil {
		t.Fatalf("SdJournalRestartData error: %v", err)
	}
	payloads := collectFacadeData(t, j, SdJournalEnumerateAvailableData)
	if !containsPayload(payloads, []byte("REPEAT=one")) ||
		!containsPayload(payloads, []byte("REPEAT=two")) ||
		!containsPayload(payloads, []byte("BIN=\x00\xff")) {
		t.Fatalf("data enumeration did not preserve repeated/binary payloads: %q", payloads)
	}
	data, err := SdJournalGetData(j, "REPEAT")
	if err != nil {
		t.Fatalf("SdJournalGetData error: %v", err)
	}
	if !bytes.Equal(data, []byte("REPEAT=one")) {
		t.Fatalf("GetData(REPEAT) = %q", data)
	}

	if err := SdJournalQueryUniqueState(j, "REPEAT"); err != nil {
		t.Fatalf("SdJournalQueryUniqueState error: %v", err)
	}
	unique := collectFacadeData(t, j, SdJournalEnumerateAvailableUnique)
	if !containsPayload(unique, []byte("REPEAT=one")) ||
		!containsPayload(unique, []byte("REPEAT=two")) ||
		!containsPayload(unique, []byte("REPEAT=three")) {
		t.Fatalf("unique enumeration missing values: %q", unique)
	}

	if err := SdJournalRestartFields(j); err != nil {
		t.Fatalf("SdJournalRestartFields error: %v", err)
	}
	fields := map[string]bool{}
	for {
		field, ok, err := SdJournalEnumerateField(j)
		if err != nil {
			t.Fatalf("SdJournalEnumerateField error: %v", err)
		}
		if !ok {
			break
		}
		fields[field] = true
	}
	for _, field := range []string{"MESSAGE", "REPEAT", "BIN"} {
		if !fields[field] {
			t.Fatalf("field %s missing from stateful field enumeration: %v", field, fields)
		}
	}

	if err := SdJournalSeekRealtimeUsec(j, 1001); err != nil {
		t.Fatalf("SdJournalSeekRealtimeUsec forward error: %v", err)
	}
	if n, err := SdJournalNext(j); err != nil || n != 1 {
		t.Fatalf("Next after seek realtime = %d, %v", n, err)
	}
	entry, err := SdJournalGetEntry(j)
	if err != nil {
		t.Fatalf("GetEntry after seek realtime error: %v", err)
	}
	if got := string(entry.Fields["MESSAGE"]); got != "second" {
		t.Fatalf("seek realtime forward landed on %q", got)
	}

	if err := SdJournalSeekRealtimeUsec(j, 1001); err != nil {
		t.Fatalf("SdJournalSeekRealtimeUsec backward error: %v", err)
	}
	if n, err := SdJournalPrevious(j); err != nil || n != 1 {
		t.Fatalf("Previous after seek realtime = %d, %v", n, err)
	}
	entry, err = SdJournalGetEntry(j)
	if err != nil {
		t.Fatalf("GetEntry after backward seek realtime error: %v", err)
	}
	if got := string(entry.Fields["MESSAGE"]); got != "second" {
		t.Fatalf("seek realtime backward landed on %q", got)
	}

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

	var messages []string
	for {
		n, err := SdJournalNext(multi)
		if err != nil {
			t.Fatalf("multi next error: %v", err)
		}
		if n == 0 {
			break
		}
		entry, err := SdJournalGetEntry(multi)
		if err != nil {
			t.Fatalf("multi get entry error: %v", err)
		}
		messages = append(messages, string(entry.Fields["MESSAGE"]))
	}
	// systemd compares same-source seqnums before realtime when interleaving files.
	wantMessages := []string{"first", "third", "second"}
	if len(messages) != len(wantMessages) {
		t.Fatalf("multi messages = %v, want %v", messages, wantMessages)
	}
	for i := range wantMessages {
		if messages[i] != wantMessages[i] {
			t.Fatalf("multi messages = %v, want %v", messages, wantMessages)
		}
	}

	if err := SdJournalSeekRealtimeUsec(multi, 1002); err != nil {
		t.Fatalf("multi seek realtime backward error: %v", err)
	}
	if n, err := SdJournalPrevious(multi); err != nil || n != 1 {
		t.Fatalf("multi previous after seek = %d, %v", n, err)
	}
	entry, err = SdJournalGetEntry(multi)
	if err != nil {
		t.Fatalf("multi get entry after backward seek error: %v", err)
	}
	if got := string(entry.Fields["MESSAGE"]); got != "second" {
		t.Fatalf("multi backward seek landed on %q", got)
	}

	if err := SdJournalSeekRealtimeUsec(multi, 999); err != nil {
		t.Fatalf("multi seek before range error: %v", err)
	}
	if n, err := SdJournalPrevious(multi); err != nil || n != 0 {
		t.Fatalf("multi previous before range = %d, %v", n, err)
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
		if err := w.Append([]Field{StringField("MESSAGE", tc.message)}, EntryOptions{RealtimeUsec: tc.realtime}); err != nil {
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
