package journal

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// The goldens under tests/output were produced by systemd v262 journalctl
// from entry-output.journal (see tests/output/README.md).
var entryOutputDir = filepath.Join("..", "..", "tests", "output")

var entryOutputFields = []string{"MESSAGE", "MULTI", "BIN", "__CURSOR", "_BOOT_ID", "AFTER", "LAST"}

func entryOutputEntries(t *testing.T) []*Entry {
	t.Helper()
	r := mustOpenReaderFile(t, filepath.Join(entryOutputDir, "entry-output.journal.zst"))
	defer r.Close()
	var entries []*Entry
	for {
		ok, err := r.Step()
		if err != nil {
			t.Fatalf("Next: %v", err)
		}
		if !ok {
			return entries
		}
		entry, err := r.GetEntry()
		if err != nil {
			t.Fatalf("GetEntry: %v", err)
		}
		entries = append(entries, entry)
	}
}

func readEntryOutputGolden(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(entryOutputDir, name))
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func decodeJSONStream(t *testing.T, data []byte) []map[string]interface{} {
	t.Helper()
	var out []map[string]interface{}
	decoder := json.NewDecoder(bytes.NewReader(data))
	for {
		var object map[string]interface{}
		if err := decoder.Decode(&object); err == io.EOF {
			return out
		} else if err != nil {
			t.Fatalf("decode JSON: %v", err)
		}
		out = append(out, object)
	}
}

func TestEntryEncoderExportMatchesSystemd(t *testing.T) {
	entries := entryOutputEntries(t)
	for _, tc := range []struct {
		golden  string
		options EntryOutputOptions
	}{
		{"export.golden", EntryOutputOptions{}},
		{"export-fields.golden", EntryOutputOptions{Fields: entryOutputFields}},
	} {
		encoder := NewEntryEncoder(tc.options)
		var got []byte
		for _, entry := range entries {
			got = encoder.AppendExport(got, entry)
		}
		if want := readEntryOutputGolden(t, tc.golden); !bytes.Equal(got, want) {
			t.Errorf("%s: export differs from systemd\n got: %q\nwant: %q", tc.golden, truncateForLog(got), truncateForLog(want))
		}
	}
}

func TestEntryEncoderJSONMatchesSystemd(t *testing.T) {
	entries := entryOutputEntries(t)
	for _, tc := range []struct {
		golden  string
		options EntryOutputOptions
		pretty  bool
	}{
		{"json.golden", EntryOutputOptions{}, false},
		{"json-all.golden", EntryOutputOptions{ShowAll: true}, false},
		{"json-fields.golden", EntryOutputOptions{Fields: entryOutputFields}, false},
		{"json-pretty.golden", EntryOutputOptions{}, true},
	} {
		encoder := NewEntryEncoder(tc.options)
		var got []byte
		for _, entry := range entries {
			if tc.pretty {
				got = encoder.AppendJSONPretty(got, entry)
			} else {
				got = encoder.AppendJSON(got, entry)
			}
		}
		// systemd orders keys by hash table iteration, so compare values.
		if gotObjects, wantObjects := decodeJSONStream(t, got), decodeJSONStream(t, readEntryOutputGolden(t, tc.golden)); !reflect.DeepEqual(gotObjects, wantObjects) {
			t.Errorf("%s: JSON differs from systemd\n got: %v\nwant: %v", tc.golden, gotObjects, wantObjects)
		}
		if !tc.pretty && bytes.Count(got, []byte("\n")) != len(entries) {
			t.Errorf("%s: compact JSON is not one line per entry", tc.golden)
		}
	}
}

func truncateForLog(data []byte) []byte {
	if len(data) > 2048 {
		return data[:2048]
	}
	return data
}

func entryOutputSyntheticEntry() *Entry {
	return &Entry{
		Payloads: [][]byte{
			[]byte("MESSAGE=say \"hi\"\t\\ <b>\n"),
			[]byte("BIN=\x00\x1f"),
			[]byte("MULTI=one"),
			[]byte("MULTI=two"),
		},
		Seqnum:    7,
		Realtime:  1234,
		Monotonic: 5678,
		BootID:    id128Fill(0x22),
		Cursor:    "s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1",
	}
}

func id128Fill(b byte) UUID {
	var u UUID
	for i := range u {
		u[i] = b
	}
	return u
}

func TestEntryEncoderJSONBytes(t *testing.T) {
	encoder := NewEntryEncoder(EntryOutputOptions{})
	got := string(encoder.AppendJSON(nil, entryOutputSyntheticEntry()))
	want := `{"BIN":[0,31],"MESSAGE":"say \"hi\"\t\\ <b>\n","MULTI":["one","two"],` +
		`"_BOOT_ID":"22222222222222222222222222222222",` +
		`"__CURSOR":"s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1",` +
		`"__MONOTONIC_TIMESTAMP":"5678","__REALTIME_TIMESTAMP":"1234","__SEQNUM":"7",` +
		`"__SEQNUM_ID":"33333333333333333333333333333333"}` + "\n"
	if got != want {
		t.Fatalf("compact JSON\n got: %s\nwant: %s", got, want)
	}

	got = string(encoder.AppendJSONPretty(nil, entryOutputSyntheticEntry()))
	want = "{\n" +
		"\t\"BIN\" : [\n\t\t0,\n\t\t31\n\t],\n" +
		"\t\"MESSAGE\" : \"say \\\"hi\\\"\\t\\\\ <b>\\n\",\n" +
		"\t\"MULTI\" : [\n\t\t\"one\",\n\t\t\"two\"\n\t],\n" +
		"\t\"_BOOT_ID\" : \"22222222222222222222222222222222\",\n" +
		"\t\"__CURSOR\" : \"s=33333333333333333333333333333333;i=7;b=22222222222222222222222222222222;m=162e;t=4d2;x=1\",\n" +
		"\t\"__MONOTONIC_TIMESTAMP\" : \"5678\",\n" +
		"\t\"__REALTIME_TIMESTAMP\" : \"1234\",\n" +
		"\t\"__SEQNUM\" : \"7\",\n" +
		"\t\"__SEQNUM_ID\" : \"33333333333333333333333333333333\"\n" +
		"}\n"
	if got != want {
		t.Fatalf("pretty JSON\n got: %s\nwant: %s", got, want)
	}
}

func TestEntryEncoderExportStopsAtPayloadWithoutSeparator(t *testing.T) {
	entry := entryOutputSyntheticEntry()
	entry.Payloads = [][]byte{[]byte("FIRST=1"), []byte("NOSEPARATOR"), []byte("AFTER=2")}
	got := string(NewEntryEncoder(EntryOutputOptions{}).AppendExport(nil, entry))
	if !bytes.HasSuffix([]byte(got), []byte("_BOOT_ID=22222222222222222222222222222222\nFIRST=1\n\n")) {
		t.Fatalf("export did not stop at the payload without '=':\n%q", got)
	}
	// JSON skips the bad payload and continues.
	json := string(NewEntryEncoder(EntryOutputOptions{}).AppendJSON(nil, entry))
	if !bytes.Contains([]byte(json), []byte(`"AFTER":"2"`)) {
		t.Fatalf("JSON did not continue past the payload without '=':\n%s", json)
	}
}

func TestEntryEncoderMapFallbackAndMissingCursor(t *testing.T) {
	entry := &Entry{
		FieldValues: map[string][][]byte{"B": {[]byte("2")}, "A": {[]byte("1")}},
		Fields:      map[string][]byte{"C": []byte("3")},
	}
	got := string(NewEntryEncoder(EntryOutputOptions{}).AppendExport(nil, entry))
	want := "__CURSOR=\n__REALTIME_TIMESTAMP=0\n__MONOTONIC_TIMESTAMP=0\n__SEQNUM=0\n" +
		"__SEQNUM_ID=00000000000000000000000000000000\n_BOOT_ID=00000000000000000000000000000000\n" +
		"A=1\nB=2\nC=3\n\n"
	if got != want {
		t.Fatalf("export of a hand-built entry\n got: %q\nwant: %q", got, want)
	}
}

func TestJSONEntryKeepsFullValues(t *testing.T) {
	entry := entryOutputSyntheticEntry()
	entry.Payloads = append(entry.Payloads, append([]byte("BIG="), bytes.Repeat([]byte("x"), 5000)...))
	object, err := JSONEntry(entry)
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := object["BIG"].(string); !ok || len(got) != 5000 {
		t.Fatalf("BIG = %T, want the full 5000-byte string", object["BIG"])
	}
	if got, ok := object["BIN"].([]int); !ok || !reflect.DeepEqual(got, []int{0, 31}) {
		t.Fatalf("BIN = %#v, want []int{0, 31}", object["BIN"])
	}
	if got, ok := object["MULTI"].([]interface{}); !ok || len(got) != 2 {
		t.Fatalf("MULTI = %#v, want two values", object["MULTI"])
	}
}

func TestIsPrintableText(t *testing.T) {
	for _, tc := range []struct {
		value        string
		withNewline  bool
		withoutCheck bool
	}{
		{"plain text", true, true},
		{"tab\there", true, true},
		{"line\nbreak", true, false},
		{"", true, true},
		{"héllo ☃ \U0001F600", true, true},
		{"�", true, true},
		{"\x00", false, false},
		{"\r", false, false},
		{"\x7f", false, false},
		{"\u0085", false, false},
		{"﷐", false, false},
		{"﷯", false, false},
		{"\xef\xbf\xbe", false, false},
		{"\xef\xbf\xbf", false, false},
		{"\xf0\x9f\xbf\xbe", false, false},
		{"\xed\xa0\x80", false, false},
		{"\xc0\x80", false, false},
		{"\xff", false, false},
		{"\xe2\x98", false, false},
	} {
		if got := IsPrintableText([]byte(tc.value), true); got != tc.withNewline {
			t.Errorf("IsPrintableText(%q, true) = %v, want %v", tc.value, got, tc.withNewline)
		}
		if got := IsPrintableText([]byte(tc.value), false); got != tc.withoutCheck {
			t.Errorf("IsPrintableText(%q, false) = %v, want %v", tc.value, got, tc.withoutCheck)
		}
	}
}

// ExportEntry keeps the SOW-0043 contract: fields with invalid or non-UTF-8
// names stay in the output instead of ending the entry.
func TestExportEntryKeepsInvalidFieldNames(t *testing.T) {
	entry := entryOutputSyntheticEntry()
	entry.Payloads = [][]byte{
		[]byte("FIRST=1"),
		[]byte("lower=x"),
		[]byte("\xffRAW=invalid utf8"),
		[]byte("NOSEPARATOR"),
		[]byte("AFTER=2"),
	}
	got := ExportEntry(entry)
	want := "_BOOT_ID=22222222222222222222222222222222\nFIRST=1\nlower=x\n\xffRAW\n\x0c\x00\x00\x00\x00\x00\x00\x00invalid utf8\nAFTER=2\n\n"
	if !bytes.HasSuffix([]byte(got), []byte(want)) {
		t.Fatalf("ExportEntry dropped fields\n got: %q\nwant suffix: %q", got, want)
	}
	strict := string(NewEntryEncoder(EntryOutputOptions{}).AppendExport(nil, entry))
	if !bytes.HasSuffix([]byte(strict), []byte("FIRST=1\n\n")) {
		t.Fatalf("EntryEncoder did not stop at the invalid name: %q", strict)
	}
}

// A caller-built cursor must not break the lossless record framing. Paired
// with Rust export_entry_bytes_length_prefixes_unsafe_metadata.
func TestExportEntryLengthPrefixesUnsafeMetadata(t *testing.T) {
	entry := entryOutputSyntheticEntry()
	entry.Cursor = "a\nb"
	got := ExportEntry(entry)
	if want := "__CURSOR\n\x03\x00\x00\x00\x00\x00\x00\x00a\nb\n"; !bytes.HasPrefix([]byte(got), []byte(want)) {
		t.Fatalf("ExportEntry metadata\n got: %q\nwant prefix: %q", got, want)
	}
}

// Paired with Rust map_fallback_skips_field_without_values.
func TestMapFallbackSkipsFieldWithoutValues(t *testing.T) {
	entry := entryOutputSyntheticEntry()
	entry.Payloads = nil
	entry.FieldValues = map[string][][]byte{"EMPTY": {}, "KEPT": {[]byte("1")}}
	entry.Fields = map[string][]byte{"FALLBACK": []byte("2")}
	got := string(NewEntryEncoder(EntryOutputOptions{}).AppendExport(nil, entry))
	if strings.Contains(got, "EMPTY") {
		t.Fatalf("empty FieldValues slice produced a field: %q", got)
	}
	for _, want := range []string{"KEPT=1\n", "FALLBACK=2\n"} {
		if !strings.Contains(got, want) {
			t.Fatalf("output %q lacks %q", got, want)
		}
	}
}
