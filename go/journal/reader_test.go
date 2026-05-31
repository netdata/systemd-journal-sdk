package journal

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestReaderOpenFile(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	for i := 0; i < 5; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", "test-message"),
			StringField("PRIORITY", "6"),
		}, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}

	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	count := 0
	for {
		if err := r.Next(); err != nil {
			if err == errEndOfEntries {
				break
			}
			t.Fatalf("Next error: %v", err)
		}
		entry, err := r.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry error: %v", err)
		}
		if entry == nil {
			t.Fatal("GetEntry returned nil entry")
		}
		if msg := string(entry.Fields["MESSAGE"]); msg != "test-message" {
			t.Errorf("MESSAGE = %q, want %q", msg, "test-message")
		}
		count++
	}

	if count != 5 {
		t.Errorf("read %d entries, want 5", count)
	}
}

func TestReaderRawFieldPayloadAPIs(t *testing.T) {
	rawName := []byte{0xff, 'R', 'A', 'W'}
	rawValue := []byte{'v', 0, '=', 'x'}
	rawPayload := append(append(append([]byte(nil), rawName...), '='), rawValue...)

	path := filepath.Join(t.TempDir(), "raw-reader.journal")
	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyRaw
	opts.Compact = true
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.AppendRaw([][]byte{
		[]byte("MESSAGE=raw reader"),
		rawPayload,
		[]byte("BINARY=a\x00=b"),
	}, EntryOptions{RealtimeUsec: 1_700_003_000_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("AppendRaw() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	for _, accessMode := range []ReaderAccessMode{ReaderAccessReadAt, ReaderAccessMmap} {
		t.Run(accessModeName(accessMode), func(t *testing.T) {
			r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(accessMode))
			if err != nil {
				t.Fatalf("OpenFileWithOptions() error = %v", err)
			}
			defer r.Close()
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("Step() = %v, %v", ok, err)
			}

			entry, err := r.GetEntry()
			if err != nil {
				t.Fatalf("GetEntry() error = %v", err)
			}
			if _, ok := entry.Fields[string(rawName)]; ok {
				t.Fatalf("invalid UTF-8 raw name leaked into UTF-8 Fields map")
			}
			if got, ok := entry.Raw(rawName); !ok || !bytes.Equal(got, rawValue) {
				t.Fatalf("Entry.Raw(%x) = %q, %v; want %q, true", rawName, got, ok, rawValue)
			}
			if got := entry.RawFieldValues[hex.EncodeToString(rawName)]; len(got) != 1 || !bytes.Equal(got[0], rawValue) {
				t.Fatalf("RawFieldValues[%x] = %q", rawName, got)
			}

			payload, ok, err := r.GetEntryPayload(rawName)
			if err != nil || !ok || !bytes.Equal(payload, rawPayload) {
				t.Fatalf("GetEntryPayload(%x) = %q, %v, %v", rawName, payload, ok, err)
			}
			value, ok, err := r.GetRaw(rawName)
			if err != nil || !ok || !bytes.Equal(value, rawValue) {
				t.Fatalf("GetRaw(%x) = %q, %v, %v", rawName, value, ok, err)
			}
			values, err := r.GetRawValues([]byte("BINARY"))
			if err != nil || len(values) != 1 || !bytes.Equal(values[0], []byte("a\x00=b")) {
				t.Fatalf("GetRawValues(BINARY) = %q, %v", values, err)
			}

			var visited [][]byte
			if err := r.VisitEntryPayloads(func(payload []byte) error {
				visited = append(visited, append([]byte(nil), payload...))
				return nil
			}); err != nil {
				t.Fatalf("VisitEntryPayloads() error = %v", err)
			}
			if !readerTestContainsPayload(visited, rawPayload) {
				t.Fatalf("VisitEntryPayloads() did not include raw payload %q in %q", rawPayload, visited)
			}

			if err := r.EntryDataRestart(); err != nil {
				t.Fatalf("EntryDataRestart() error = %v", err)
			}
			var enumerated [][]byte
			for {
				payload, ok, err := r.EnumerateEntryPayload()
				if err != nil {
					t.Fatalf("EnumerateEntryPayload() error = %v", err)
				}
				if !ok {
					break
				}
				enumerated = append(enumerated, append([]byte(nil), payload...))
			}
			if !readerTestContainsPayload(enumerated, rawPayload) {
				t.Fatalf("EnumerateEntryPayload() did not include raw payload %q in %q", rawPayload, enumerated)
			}

			j, err := SdJournalOpenFileWithOptions(path, 0, DefaultReaderOptions().WithAccessMode(accessMode))
			if err != nil {
				t.Fatalf("SdJournalOpenFileWithOptions() error = %v", err)
			}
			defer SdJournalClose(j)
			if n, err := SdJournalNext(j); err != nil || n != 1 {
				t.Fatalf("SdJournalNext() = %d, %v", n, err)
			}
			facadePayload, err := SdJournalGetData(j, string(rawName))
			if err != nil || !bytes.Equal(facadePayload, rawPayload) {
				t.Fatalf("SdJournalGetData(raw) = %q, %v", facadePayload, err)
			}
		})
	}
}

func TestReaderPayloadEnumerationReusesOffsetsAcrossEntries(t *testing.T) {
	rows := []struct {
		fields []Field
		want   [][]byte
	}{
		{
			fields: []Field{
				StringField("MESSAGE", "one"),
				StringField("A", "1"),
				StringField("B", "1"),
			},
			want: [][]byte{
				[]byte("MESSAGE=one"),
				[]byte("A=1"),
				[]byte("B=1"),
			},
		},
		{
			fields: []Field{
				StringField("MESSAGE", "two"),
			},
			want: [][]byte{
				[]byte("MESSAGE=two"),
			},
		},
		{
			fields: []Field{
				StringField("MESSAGE", "three"),
				StringField("A", "3"),
				StringField("C", "3"),
				StringField("D", "3"),
				StringField("E", "3"),
			},
			want: [][]byte{
				[]byte("MESSAGE=three"),
				[]byte("A=3"),
				[]byte("C=3"),
				[]byte("D=3"),
				[]byte("E=3"),
			},
		},
	}

	for _, compact := range []bool{false, true} {
		name := "regular"
		if compact {
			name = "compact"
		}
		t.Run(name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "payload-reuse.journal")
			opts := testOptions()
			opts.Compact = compact
			w, err := Create(path, opts)
			if err != nil {
				t.Fatalf("Create() error = %v", err)
			}
			for i, row := range rows {
				if err := w.Append(row.fields, EntryOptions{
					RealtimeUsec:  1_700_005_000_000_000 + uint64(i),
					MonotonicUsec: uint64(i + 1),
				}); err != nil {
					t.Fatalf("Append(%d) error = %v", i, err)
				}
			}
			if err := w.Close(); err != nil {
				t.Fatalf("Close() error = %v", err)
			}

			for _, accessMode := range []ReaderAccessMode{ReaderAccessReadAt, ReaderAccessMmap} {
				t.Run(accessModeName(accessMode), func(t *testing.T) {
					r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(accessMode))
					if err != nil {
						t.Fatalf("OpenFileWithOptions() error = %v", err)
					}
					defer r.Close()

					for i, row := range rows {
						if ok, err := r.Step(); err != nil || !ok {
							t.Fatalf("Step(%d) = %v, %v; want true, nil", i, ok, err)
						}

						var visited [][]byte
						if err := r.VisitEntryPayloads(func(payload []byte) error {
							visited = append(visited, append([]byte(nil), payload...))
							return nil
						}); err != nil {
							t.Fatalf("VisitEntryPayloads(%d) error = %v", i, err)
						}
						readerTestPayloadSetMatches(t, visited, row.want)

						if err := r.EntryDataRestart(); err != nil {
							t.Fatalf("EntryDataRestart(%d) error = %v", i, err)
						}
						var enumerated [][]byte
						for {
							payload, ok, err := r.EnumerateEntryPayload()
							if err != nil {
								t.Fatalf("EnumerateEntryPayload(%d) error = %v", i, err)
							}
							if !ok {
								break
							}
							enumerated = append(enumerated, append([]byte(nil), payload...))
						}
						readerTestPayloadSetMatches(t, enumerated, row.want)

						if err := r.EntryDataRestart(); err != nil {
							t.Fatalf("EntryDataRestart(%d repeat) error = %v", i, err)
						}
						var repeated [][]byte
						for {
							payload, ok, err := r.EnumerateEntryPayload()
							if err != nil {
								t.Fatalf("EnumerateEntryPayload(%d repeat) error = %v", i, err)
							}
							if !ok {
								break
							}
							repeated = append(repeated, append([]byte(nil), payload...))
						}
						readerTestPayloadSetMatches(t, repeated, row.want)
					}

					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("final Step() = %v, %v; want false, nil", ok, err)
					}
				})
			}
		})
	}
}

func TestReaderBoundsControlLiveRefresh(t *testing.T) {
	for _, accessMode := range []ReaderAccessMode{ReaderAccessReadAt, ReaderAccessMmap} {
		t.Run(accessModeName(accessMode), func(t *testing.T) {
			livePath := filepath.Join(t.TempDir(), "live.journal")
			w, err := Create(livePath, testOptions())
			if err != nil {
				t.Fatalf("Create(live) error = %v", err)
			}
			if err := w.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{RealtimeUsec: 1_700_004_000_000_000, MonotonicUsec: 1}); err != nil {
				t.Fatalf("Append(first) error = %v", err)
			}
			r, err := OpenFileWithOptions(livePath, DefaultReaderOptions().WithAccessMode(accessMode))
			if err != nil {
				t.Fatalf("OpenFileWithOptions(live) error = %v", err)
			}
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("live Step(first) = %v, %v", ok, err)
			}
			if ok, err := r.Step(); err != nil || ok {
				t.Fatalf("live Step(eof) = %v, %v; want false, nil", ok, err)
			}
			if err := w.Append([]Field{StringField("MESSAGE", "second")}, EntryOptions{RealtimeUsec: 1_700_004_000_000_001, MonotonicUsec: 2}); err != nil {
				t.Fatalf("Append(second) error = %v", err)
			}
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("live Step(after append) = %v, %v; want true, nil", ok, err)
			}
			entry, err := r.GetEntry()
			if err != nil {
				t.Fatalf("GetEntry(second) error = %v", err)
			}
			if got := string(entry.Fields["MESSAGE"]); got != "second" {
				t.Fatalf("live refreshed MESSAGE = %q, want second", got)
			}
			_ = r.Close()
			_ = w.Close()

			snapshotPath := filepath.Join(t.TempDir(), "snapshot.journal")
			w, err = Create(snapshotPath, testOptions())
			if err != nil {
				t.Fatalf("Create(snapshot) error = %v", err)
			}
			if err := w.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{RealtimeUsec: 1_700_004_100_000_000, MonotonicUsec: 1}); err != nil {
				t.Fatalf("Append(snapshot first) error = %v", err)
			}
			r, err = OpenFileWithOptions(snapshotPath, DefaultReaderOptions().WithAccessMode(accessMode).WithSnapshot(true))
			if err != nil {
				t.Fatalf("OpenFileWithOptions(snapshot) error = %v", err)
			}
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("snapshot Step(first) = %v, %v", ok, err)
			}
			if err := w.Append([]Field{StringField("MESSAGE", "second")}, EntryOptions{RealtimeUsec: 1_700_004_100_000_001, MonotonicUsec: 2}); err != nil {
				t.Fatalf("Append(snapshot second) error = %v", err)
			}
			if ok, err := r.Step(); err != nil || ok {
				t.Fatalf("snapshot Step(after append) = %v, %v; want false, nil", ok, err)
			}
			_ = r.Close()
			_ = w.Close()
		})
	}
}

func accessModeName(mode ReaderAccessMode) string {
	if mode == ReaderAccessMmap {
		return "mmap"
	}
	return "read-at"
}

func readerTestContainsPayload(payloads [][]byte, want []byte) bool {
	for _, payload := range payloads {
		if bytes.Equal(payload, want) {
			return true
		}
	}
	return false
}

func readerTestPayloadSetMatches(t *testing.T, got [][]byte, want [][]byte) {
	t.Helper()

	if len(got) != len(want) {
		t.Fatalf("payload count = %d, want %d; got %q want %q", len(got), len(want), got, want)
	}
	for _, payload := range want {
		if !readerTestContainsPayload(got, payload) {
			t.Fatalf("payloads %q did not include %q", got, payload)
		}
	}
}

func TestReaderSystemdZstdFixture(t *testing.T) {
	path := filepath.Join("..", "..", "fixtures", "systemd", "test-data", "no-rtc", "system.journal.zst")

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile systemd fixture: %v", err)
	}
	defer r.Close()

	count := 0
	var sawTransport bool
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
		if string(entry.Fields["_TRANSPORT"]) != "" {
			sawTransport = true
		}
		if count == 0 {
			if got := string(entry.Fields["_TRANSPORT"]); got != "kernel" {
				t.Fatalf("first _TRANSPORT = %q, want kernel", got)
			}
			if got := string(entry.Fields["MESSAGE"]); !strings.HasPrefix(got, "Booting Linux") {
				t.Fatalf("first MESSAGE = %q, want Booting Linux prefix", got)
			}
		}
		count++
		if count >= 100 {
			break
		}
	}
	if count == 0 {
		t.Fatal("systemd fixture produced no entries")
	}
	if !sawTransport {
		t.Fatal("systemd fixture did not expose _TRANSPORT in first 100 entries")
	}
}

func TestReaderIteration(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	for i := 0; i < 10; i++ {
		if err := w.Append([]Field{
			StringField("SEQ", string(rune('0'+i))),
		}, EntryOptions{
			RealtimeUsec: uint64(1000 + i),
		}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.SeekHead()
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
	if count != 10 {
		t.Errorf("Step read %d entries, want 10", count)
	}

	r.SeekTail()
	count = 0
	for {
		ok, err := r.StepBack()
		if err != nil {
			t.Fatalf("StepBack error: %v", err)
		}
		if !ok {
			break
		}
		count++
	}
	if count != 10 {
		t.Errorf("StepBack read %d entries, want 10", count)
	}
}

func TestReaderMatchSameFieldOR(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	entries := []struct {
		msg string
		pr  string
	}{
		{"alpha", "3"},
		{"beta", "6"},
		{"gamma", "3"},
		{"delta", "6"},
	}

	for _, e := range entries {
		if err := w.Append([]Field{
			StringField("MESSAGE", e.msg),
			StringField("PRIORITY", e.pr),
		}, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.AddMatch([]byte("MESSAGE=alpha"))
	r.AddMatch([]byte("MESSAGE=beta"))

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

	if count != 2 {
		t.Errorf("matched %d entries, want 2 (MESSAGE=alpha OR MESSAGE=beta)", count)
	}
}

func TestReaderMatchAND(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	entries := []struct {
		msg string
		pr  string
	}{
		{"alpha", "3"},
		{"beta", "6"},
		{"gamma", "3"},
		{"delta", "6"},
	}

	for _, e := range entries {
		if err := w.Append([]Field{
			StringField("MESSAGE", e.msg),
			StringField("PRIORITY", e.pr),
		}, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.AddMatch([]byte("PRIORITY=3"))
	r.AddMatch([]byte("MESSAGE=alpha"))

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

	if count != 1 {
		t.Errorf("matched %d entries, want 1 (PRIORITY=3 AND MESSAGE=alpha)", count)
	}
}

func TestReaderMatchDisjunction(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	entries := []struct {
		l3 string
		l4 string
	}{
		{"ok", "no"},
		{"yes", "no"},
		{"no", "ok"},
		{"no", "yes"},
	}

	for _, e := range entries {
		if err := w.Append([]Field{
			StringField("L3", e.l3),
			StringField("L4", e.l4),
		}, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.AddMatch([]byte("L3=ok"))
	r.AddDisjunction()
	r.AddMatch([]byte("L4=yes"))

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

	if count != 2 {
		t.Errorf("matched %d entries, want 2 (L3=ok + L4=yes)", count)
	}
}

func TestReaderSystemdComplexMatchExpression(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	entries := [][]Field{
		{
			StringField("L3", "ok"),
			StringField("TWO", "two"),
			StringField("ONE", "one"),
		},
		{
			StringField("L4_1", "yes"),
			StringField("L4_2", "ok"),
			StringField("PIFF", "paff"),
			StringField("QUUX", "xxxxx"),
			StringField("HALLO", "WALDO"),
			{Name: "B", Value: []byte{'C', 0, 'D'}},
			{Name: "A", Value: []byte{1, 2}},
		},
		{
			StringField("L3", "ok"),
		},
		{
			StringField("TWO", "two"),
			StringField("ONE", "one"),
		},
	}
	for _, fields := range entries {
		if err := w.Append(fields, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	addSystemdComplexMatchExpression(r)

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
	if count != 2 {
		t.Fatalf("matched %d entries, want 2", count)
	}
}

func addSystemdComplexMatchExpression(r interface {
	AddMatch([]byte)
	AddDisjunction()
	AddConjunction()
}) {
	r.AddMatch([]byte{'A', '=', 1, 2})
	r.AddMatch([]byte{'B', '=', 'C', 0, 'D'})
	r.AddMatch([]byte("HALLO=WALDO"))
	r.AddMatch([]byte("QUUX=mmmm"))
	r.AddMatch([]byte("QUUX=xxxxx"))
	r.AddMatch([]byte("HALLO="))
	r.AddMatch([]byte("QUUX=xxxxx"))
	r.AddMatch([]byte("QUUX=yyyyy"))
	r.AddMatch([]byte("PIFF=paff"))
	r.AddDisjunction()
	r.AddMatch([]byte("ONE=one"))
	r.AddMatch([]byte("ONE=two"))
	r.AddMatch([]byte("TWO=two"))
	r.AddConjunction()
	r.AddMatch([]byte("L4_1=yes"))
	r.AddMatch([]byte("L4_1=ok"))
	r.AddMatch([]byte("L4_2=yes"))
	r.AddMatch([]byte("L4_2=ok"))
	r.AddDisjunction()
	r.AddMatch([]byte("L3=yes"))
	r.AddMatch([]byte("L3=ok"))
}

func TestReaderBinaryFields(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	binaryValue := []byte{0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE}
	if err := w.Append([]Field{
		{Name: "BINARY", Value: binaryValue},
		{Name: "STRING", Value: []byte("hello")},
	}, EntryOptions{}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.SeekHead()
	r.Step()

	entry, err := r.GetEntry()
	if err != nil {
		t.Fatalf("GetEntry error: %v", err)
	}

	if !bytes.Equal(entry.Fields["BINARY"], binaryValue) {
		t.Errorf("BINARY = %v, want %v", entry.Fields["BINARY"], binaryValue)
	}
	if string(entry.Fields["STRING"]) != "hello" {
		t.Errorf("STRING = %q, want %q", string(entry.Fields["STRING"]), "hello")
	}
}

func TestExportEntryBinaryEncodingAndSeparator(t *testing.T) {
	entry := &Entry{
		Fields: map[string][]byte{
			"MESSAGE":  []byte("plain"),
			"BINARY":   []byte{0x00, 0x01, '\n', 0xff},
			"_BOOT_ID": []byte("actual-field-boot-id"),
		},
		FieldValues: map[string][][]byte{
			"MESSAGE":  {[]byte("plain")},
			"BINARY":   {[]byte{0x00, 0x01, '\n', 0xff}},
			"_BOOT_ID": {[]byte("actual-field-boot-id")},
		},
		Seqnum:    7,
		Realtime:  1234,
		Monotonic: 5678,
		BootID:    testBootID,
		Cursor:    "s=303132333435363738393a3b3c3d3e3f;j=202122232425262728292a2b2c2d2e2f;c=00000000000004d2;n=7",
	}

	out := []byte(ExportEntry(entry))
	if !bytes.HasSuffix(out, []byte("\n\n")) {
		t.Fatalf("export entry does not end with blank entry separator: %q", out[len(out)-4:])
	}
	if got := bytes.Count(out, []byte("_BOOT_ID=")); got != 1 {
		t.Fatalf("export contains %d _BOOT_ID text fields, want 1:\n%s", got, out)
	}

	idx := bytes.Index(out, []byte("BINARY\n"))
	if idx < 0 {
		t.Fatalf("binary field header not found in export:\n%s", out)
	}
	payloadStart := idx + len("BINARY\n")
	if len(out) < payloadStart+8 {
		t.Fatalf("binary field missing size prefix")
	}
	size := binary.LittleEndian.Uint64(out[payloadStart : payloadStart+8])
	if size != 4 {
		t.Fatalf("binary field size = %d, want 4", size)
	}
	payload := out[payloadStart+8 : payloadStart+8+int(size)]
	if !bytes.Equal(payload, []byte{0x00, 0x01, '\n', 0xff}) {
		t.Fatalf("binary payload = %v, want [0 1 10 255]", payload)
	}
	if out[payloadStart+8+int(size)] != '\n' {
		t.Fatalf("binary payload is not newline terminated")
	}
}

func TestJSONEntryBinaryAndDuplicateFields(t *testing.T) {
	entry := &Entry{
		Fields: map[string][]byte{
			"MESSAGE":  []byte("first"),
			"BINARY":   []byte{0x00, 0xff},
			"_BOOT_ID": []byte(testBootID.String()),
		},
		FieldValues: map[string][][]byte{
			"MESSAGE":  {[]byte("first"), []byte("second")},
			"BINARY":   {[]byte{0x00, 0xff}},
			"_BOOT_ID": {[]byte(testBootID.String())},
		},
		Seqnum: 1,
		BootID: testBootID,
	}

	obj, err := JSONEntry(entry)
	if err != nil {
		t.Fatalf("JSONEntry error: %v", err)
	}
	encoded, err := json.Marshal(obj)
	if err != nil {
		t.Fatalf("Marshal JSONEntry output: %v", err)
	}

	var decoded map[string]interface{}
	if err := json.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("Unmarshal JSONEntry output: %v", err)
	}
	if _, ok := decoded["MESSAGE"].([]interface{}); !ok {
		t.Fatalf("duplicate MESSAGE was not encoded as JSON array: %#v", decoded["MESSAGE"])
	}
	binaryValue, ok := decoded["BINARY"].([]interface{})
	if !ok {
		t.Fatalf("binary field was not encoded as JSON byte array: %#v", decoded["BINARY"])
	}
	if len(binaryValue) != 2 || binaryValue[0].(float64) != 0 || binaryValue[1].(float64) != 255 {
		t.Fatalf("binary JSON value = %#v, want [0 255]", binaryValue)
	}
	if got, ok := decoded["_BOOT_ID"].(string); !ok || got != testBootID.String() {
		t.Fatalf("_BOOT_ID JSON value = %#v, want scalar %q", decoded["_BOOT_ID"], testBootID.String())
	}
}

func TestReaderCursor(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	for i := 0; i < 3; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", "msg"),
		}, EntryOptions{
			RealtimeUsec: uint64(1000 + i),
		}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.SeekHead()
	r.Step()

	cursor, err := r.GetCursor()
	if err != nil {
		t.Fatalf("GetCursor error: %v", err)
	}
	if cursor == "" {
		t.Error("GetCursor returned empty cursor")
	}

	match, err := r.TestCursor(cursor)
	if err != nil {
		t.Fatalf("TestCursor error: %v", err)
	}
	if !match {
		t.Error("TestCursor returned false for same cursor")
	}

	match, err = r.TestCursor("invalid-cursor")
	if err != nil {
		t.Fatalf("TestCursor error: %v", err)
	}
	if match {
		t.Error("TestCursor returned true for invalid cursor")
	}
}

func TestReaderUniqueFields(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	priorities := []string{"0", "3", "6", "7"}
	for _, p := range priorities {
		if err := w.Append([]Field{
			StringField("PRIORITY", p),
		}, EntryOptions{}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	values, err := r.QueryUnique("PRIORITY")
	if err != nil {
		t.Fatalf("QueryUnique error: %v", err)
	}

	if len(values) != 4 {
		t.Errorf("QueryUnique returned %d values, want 4", len(values))
	}

	r.entryOffsets = nil
	values, err = r.QueryUnique("PRIORITY")
	if err != nil {
		t.Fatalf("indexed QueryUnique error after clearing entry offsets: %v", err)
	}
	if len(values) != 4 {
		t.Fatalf("indexed QueryUnique returned %d values after clearing entry offsets, want 4", len(values))
	}

	fields, err := r.EnumerateFields()
	if err != nil {
		t.Fatalf("indexed EnumerateFields error after clearing entry offsets: %v", err)
	}
	if _, ok := fields["PRIORITY"]; !ok {
		t.Fatalf("indexed EnumerateFields missing PRIORITY after clearing entry offsets: %#v", fields)
	}
}

func TestReaderEnumerateFields(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	if err := w.Append([]Field{
		StringField("FIELD_A", "value_a"),
		StringField("FIELD_B", "value_b"),
	}, EntryOptions{}); err != nil {
		t.Fatalf("Append error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	fields, err := r.EnumerateFields()
	if err != nil {
		t.Fatalf("EnumerateFields error: %v", err)
	}

	expected := map[string]struct{}{
		"FIELD_A": {},
		"FIELD_B": {},
	}
	for f := range expected {
		if _, ok := fields[f]; !ok {
			t.Errorf("missing field %q", f)
		}
	}
}

func TestReaderCorruption(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "corrupt.journal")

	f, err := os.Create(path)
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	f.Write([]byte("not a journal"))
	f.Close()

	_, err = OpenFile(path)
	if err == nil {
		t.Error("OpenFile should fail for corrupt file")
	}
}

func TestReaderEmptyFile(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "empty.journal")

	f, err := os.Create(path)
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	f.Close()

	_, err = OpenFile(path)
	if err == nil {
		t.Error("OpenFile should fail for empty file")
	}
}

func TestDirectoryReader(t *testing.T) {
	tmp := t.TempDir()
	dir := filepath.Join(tmp, "journal.d")

	if err := os.MkdirAll(dir, 0o750); err != nil {
		t.Fatalf("MkdirAll error: %v", err)
	}

	for i := 0; i < 3; i++ {
		path := filepath.Join(dir, "system@abc123-00000001-0000000"+string(rune('0'+i))+".journal")
		w, err := Create(path, Options{})
		if err != nil {
			t.Fatalf("Create error: %v", err)
		}
		for j := 0; j < 3; j++ {
			if err := w.Append([]Field{
				StringField("INDEX", string(rune('0'+i))),
				StringField("PRIORITY", "6"),
			}, EntryOptions{
				RealtimeUsec: uint64(1000 + i*10 + j),
			}); err != nil {
				t.Fatalf("Append error: %v", err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatalf("Close error: %v", err)
		}
	}

	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatalf("OpenDirectory error: %v", err)
	}
	defer dr.Close()

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

	if count != 9 {
		t.Errorf("DirectoryReader read %d entries, want 9", count)
	}

	values, err := dr.QueryUnique("INDEX")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique INDEX error: %v", err)
	}
	if len(values) != 3 {
		t.Fatalf("DirectoryReader QueryUnique INDEX returned %d values, want 3", len(values))
	}
	values, err = dr.QueryUnique("PRIORITY")
	if err != nil {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY error: %v", err)
	}
	if len(values) != 1 || string(values[0]) != "6" {
		t.Fatalf("DirectoryReader QueryUnique PRIORITY returned %#v, want one value 6", values)
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
				RealtimeUsec: tc.realtime + uint64(i),
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

func TestParseMatchString(t *testing.T) {
	tests := []struct {
		input   string
		wantErr bool
	}{
		{"", true},
		{"=", true},
		{"=value", true},
		{"field", true},
		{"field=value", true},
		{"FIELD_NAME=value", false},
		{"_UNDERSCORE=value", false},
		{"MESSAGE=hello world", false},
	}

	for _, tt := range tests {
		_, err := ParseMatchString(tt.input)
		if (err != nil) != tt.wantErr {
			t.Errorf("ParseMatchString(%q) error = %v, wantErr %v", tt.input, err, tt.wantErr)
		}
	}
}

func TestParseCursor(t *testing.T) {
	invalid := []string{
		"invalid",
		"s=;j=def456;c=0000000000000001;n=42",
		"s=abc123;j=;c=0000000000000001;n=42",
	}
	for _, cursor := range invalid {
		_, _, _, _, err := ParseCursor(cursor)
		if err == nil {
			t.Errorf("ParseCursor(%q) should fail", cursor)
		}
	}

	seqnumID, bootID, realtime, seqnum, err := ParseCursor("s=abc123;j=def456;c=0000000000000001;n=42")
	if err != nil {
		t.Fatalf("ParseCursor error: %v", err)
	}
	if seqnumID != "abc123" {
		t.Errorf("seqnumID = %q, want %q", seqnumID, "abc123")
	}
	if seqnum != 42 {
		t.Errorf("seqnum = %d, want %d", seqnum, 42)
	}
	_ = bootID
	_ = realtime
}

func TestUnsupportedDaemonCommands(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	j, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer j.Close()

	if err := j.SeekHead(); err != nil {
		t.Fatalf("SeekHead error: %v", err)
	}
	if err := j.SeekTail(); err != nil {
		t.Fatalf("SeekTail error: %v", err)
	}
}

func TestReaderRealtimeUsec(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, Options{})
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	realtimeBase := uint64(1_700_001_000_000_000)
	for i := 0; i < 3; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", "msg"),
		}, EntryOptions{
			RealtimeUsec: realtimeBase + uint64(i),
		}); err != nil {
			t.Fatalf("Append error: %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close error: %v", err)
	}

	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile error: %v", err)
	}
	defer r.Close()

	r.SeekHead()
	for i := 0; i < 3; i++ {
		r.Step()
		rt, err := r.GetRealtimeUsec()
		if err != nil {
			t.Fatalf("GetRealtimeUsec error: %v", err)
		}
		if rt != realtimeBase+uint64(i) {
			t.Errorf("realtime = %d, want %d", rt, realtimeBase+uint64(i))
		}
	}
}
