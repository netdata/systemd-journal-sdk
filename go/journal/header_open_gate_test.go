package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

// Paired with Rust journal-core file_tests.rs header open-gate tests.

func headerGateJournal(t *testing.T, compact bool) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "gate.journal")
	opts := testOptions()
	opts.Compact = compact
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.AppendMapWithOptions(map[string]string{"MESSAGE": "gate"}, testEntryOptions(1)); err != nil {
		t.Fatalf("Append() error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	return path
}

func patchHeader(t *testing.T, path string, offset int64, value []byte) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if _, err := f.WriteAt(value, offset); err != nil {
		t.Fatal(err)
	}
}

func readFileForTest(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func TestOpenRejectsUnknownIncompatibleFlag(t *testing.T) {
	path := headerGateJournal(t, false)
	flags := make([]byte, 4)
	copy(flags, readFileForTest(t, path)[12:16])
	binary.LittleEndian.PutUint32(flags, binary.LittleEndian.Uint32(flags)|1<<5)
	patchHeader(t, path, 12, flags)
	before := readFileForTest(t, path)

	if r, err := OpenFile(path); !errors.Is(err, errUnsupportedJournal) {
		if r != nil {
			_ = r.Close()
		}
		t.Fatalf("OpenFile() error = %v, want errUnsupportedJournal", err)
	}
	if w, err := Open(path); !errors.Is(err, errUnsupportedJournal) {
		if w != nil {
			_ = w.Close()
		}
		t.Fatalf("Open() error = %v, want errUnsupportedJournal", err)
	}
	if !bytes.Equal(readFileForTest(t, path), before) {
		t.Fatal("rejected open mutated the file")
	}
}

func TestAppendOpenRequiresBothHashTables(t *testing.T) {
	for name, offset := range map[string]int64{"data": 104, "field": 120} {
		t.Run(name, func(t *testing.T) {
			path := headerGateJournal(t, false)
			patchHeader(t, path, offset, make([]byte, 16))
			before := readFileForTest(t, path)
			if w, err := Open(path); !errors.Is(err, errInvalidJournal) {
				if w != nil {
					_ = w.Close()
				}
				t.Fatalf("Open() error = %v, want errInvalidJournal", err)
			}
			if !bytes.Equal(readFileForTest(t, path), before) {
				t.Fatal("rejected append-open mutated the file")
			}
		})
	}
}

func TestAppendOpenRejectsCompactFileBeyondFourGiB(t *testing.T) {
	path := headerGateJournal(t, true)
	original := readFileForTest(t, path)
	if err := os.Truncate(path, int64(journalCompactSizeMax)+pageSize); err != nil {
		t.Fatal(err)
	}
	if w, err := Open(path); !errors.Is(err, errInvalidJournal) {
		if w != nil {
			_ = w.Close()
		}
		t.Fatalf("Open() error = %v, want errInvalidJournal", err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if uint64(info.Size()) != journalCompactSizeMax+pageSize {
		t.Fatalf("rejected append-open resized the file to %d", info.Size())
	}
	// The sparse tail beyond the original content is all zero by construction;
	// compare the written prefix rather than reading 4 GiB.
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	prefix := make([]byte, len(original))
	if _, err := f.ReadAt(prefix, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(prefix, original) {
		t.Fatal("rejected append-open mutated the file")
	}
}
