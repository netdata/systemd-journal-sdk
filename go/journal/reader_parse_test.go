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
