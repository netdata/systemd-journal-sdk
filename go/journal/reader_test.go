package journal

import (
	"bytes"
	"encoding/binary"
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
	_, _, _, _, err := ParseCursor("invalid")
	if err == nil {
		t.Error("ParseCursor should fail for invalid cursor")
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
