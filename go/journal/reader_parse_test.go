package journal

import (
	"path/filepath"
	"testing"
)

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

	seqnumID, bootID, realtime, seqnum, err = ParseCursor("s=ABC123;i=2a;b=01234567-89ab-cdef-0123-456789abcdef;m=9;t=10;x=ff")
	if err != nil {
		t.Fatalf("official ParseCursor error: %v", err)
	}
	if seqnumID != "abc123" {
		t.Errorf("official seqnumID = %q, want %q", seqnumID, "abc123")
	}
	if bootID != "0123456789abcdef0123456789abcdef" {
		t.Errorf("official bootID = %q", bootID)
	}
	if realtime != 16 {
		t.Errorf("official realtime = %d, want 16", realtime)
	}
	if seqnum != 42 {
		t.Errorf("official seqnum = %d, want 42", seqnum)
	}

	seqnumID, bootID, realtime, seqnum, err = ParseCursor("s=ABC123;i=2a")
	if err != nil {
		t.Fatalf("partial seqnum ParseCursor error: %v", err)
	}
	if seqnumID != "abc123" || bootID != "" || realtime != 0 || seqnum != 42 {
		t.Fatalf("partial seqnum cursor = (%q, %q, %d, %d)", seqnumID, bootID, realtime, seqnum)
	}

	seqnumID, bootID, realtime, seqnum, err = ParseCursor("t=10")
	if err != nil {
		t.Fatalf("partial realtime ParseCursor error: %v", err)
	}
	if seqnumID != "" || bootID != "" || realtime != 16 || seqnum != 0 {
		t.Fatalf("partial realtime cursor = (%q, %q, %d, %d)", seqnumID, bootID, realtime, seqnum)
	}
}

func TestCursorSeekOrderIgnoresXHashButExactMatchChecksIt(t *testing.T) {
	got, err := parseCursorLocation("s=abc;i=2;b=def;m=3;t=4;x=1", false)
	if err != nil {
		t.Fatalf("got cursor parse error: %v", err)
	}
	want, err := parseCursorLocation("s=abc;i=2;b=def;m=3;t=4;x=ffffffffffffffff", true)
	if err != nil {
		t.Fatalf("want cursor parse error: %v", err)
	}
	if !cursorLocationAtOrAfter(got, want) {
		t.Fatalf("seek ordering should ignore mismatched x= when other components match")
	}
	if cursorLocationMatches(got, want) {
		t.Fatalf("exact cursor matching must still include x=")
	}
}

func TestUnsupportedDaemonCommands(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "test.journal")

	w, err := Create(path, testOptions())
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

	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create error: %v", err)
	}

	realtimeBase := uint64(1_700_001_000_000_000)
	for i := 0; i < 3; i++ {
		if err := w.Append([]Field{
			StringField("MESSAGE", "msg"),
		}, EntryOptions{
			RealtimeUsec:  realtimeBase + uint64(i),
			MonotonicUsec: uint64(i + 1),
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
