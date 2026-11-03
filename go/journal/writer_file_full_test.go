package journal

import (
	"bytes"
	"errors"
	"fmt"
	"path/filepath"
	"testing"
)

// Paired with Rust file_tests.rs append_refuses_entry_that_could_reach_four_gib
// and journal-log-writer append_target.rs file-full tests. The tests move the
// in-memory append offset to the ceiling instead of writing 4 GiB.

func TestAppendRefusesEntryThatCouldReachFourGiB(t *testing.T) {
	for _, compact := range []bool{false, true} {
		t.Run(fmt.Sprintf("compact=%v", compact), func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "full.journal")
			opts := testOptions()
			opts.Compact = compact
			w, err := Create(path, opts)
			if err != nil {
				t.Fatal(err)
			}
			defer w.Close()
			if err := w.Append([]Field{StringField("MESSAGE", "first")}, testEntryOptions(1)); err != nil {
				t.Fatal(err)
			}
			fields := []Field{StringField("MESSAGE", "second")}
			bound := w.boundEntryGrowth(len(fields), func(i int) uint64 {
				return uint64(len(fields[i].Name) + 1 + len(fields[i].Value))
			}, 0)
			before := readFileForTest(t, path)
			appendOffset := w.appendOffset
			// One byte short of the room the entry may need.
			w.appendOffset = journalAppendCeiling - bound + 8
			err = w.Append(fields, testEntryOptions(2))
			if !errors.Is(err, ErrJournalFileFull) || errors.Is(err, ErrWriterFailed) {
				t.Fatalf("Append() error = %v, want ErrJournalFileFull only", err)
			}
			if !bytes.Equal(readFileForTest(t, path), before) {
				t.Fatal("refused append mutated the file")
			}
			w.appendOffset = appendOffset
			if err := w.Append(fields, testEntryOptions(2)); err != nil {
				t.Fatalf("Append() after refusal error = %v", err)
			}
		})
	}
}

func TestEntryArraysStopDoublingNearTheCeiling(t *testing.T) {
	w, err := Create(filepath.Join(t.TempDir(), "arrays.journal"), testOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	w.entryGrowthBound = 4096
	w.appendOffset = 1 << 20
	if got := w.entryArrayCapacity(1 << 10); got != 1<<10 {
		t.Fatalf("far from the ceiling: capacity = %d, want %d", got, 1<<10)
	}
	// Room for the reserved growth plus 1023 items, not 1024.
	w.appendOffset = journalAppendCeiling - w.entryGrowthBound - align8(offsetArrayObjectHeaderSize+1023*regularOffsetArrayItemSize)
	if got := w.entryArrayCapacity(1023); got != 1023 {
		t.Fatalf("exact fit: capacity = %d, want 1023", got)
	}
	if got := w.entryArrayCapacity(1024); got != 4 {
		t.Fatalf("near the ceiling: capacity = %d, want 4", got)
	}
	// Outside an append no reservation applies.
	w.entryGrowthBound = 0
	if got := w.entryArrayCapacity(1024); got != 1024 {
		t.Fatalf("outside an append: capacity = %d, want 1024", got)
	}
}

func TestLogContinuesInNewFileWhenActiveFileIsFull(t *testing.T) {
	for _, raw := range []bool{false, true} {
		t.Run(fmt.Sprintf("raw=%v", raw), func(t *testing.T) {
			root := t.TempDir()
			config := LogConfig{Options: testOptions(), Source: "system"}
			log := mustNewLogForTest(t, root, config, "log")
			appendLogEntry(t, log, "first", "", 1_700_002_276_000_000, 1)
			full := log.ActivePath()
			log.writer.appendOffset = journalAppendCeiling
			var err error
			opts := EntryOptions{RealtimeUsec: 1_700_002_276_000_001, RealtimeUsecSet: true, MonotonicUsec: 2, MonotonicUsecSet: true}
			if raw {
				err = log.AppendRaw([][]byte{[]byte("MESSAGE=second")}, opts)
			} else {
				err = log.Append([]Field{StringField("MESSAGE", "second")}, opts)
			}
			if err != nil {
				t.Fatalf("append into a full file: %v", err)
			}
			if log.ActivePath() == full {
				t.Fatal("Log did not rotate away from the full file")
			}
			assertJournalSeqnumRange(t, log.ActivePath(), "new file", 2, 2)
			assertJournalSeqnumRange(t, full, "full file", 1, 1)
			closeLogForTest(t, log, "log")
		})
	}
}

func TestLogRefusesEntryThatDoesNotFitAnEmptyFile(t *testing.T) {
	root := t.TempDir()
	config := LogConfig{Options: testOptions(), Source: "system", OpenMode: LogOpenEager}
	log := mustNewLogForTest(t, root, config, "log")
	empty := log.ActivePath()
	log.writer.appendOffset = journalAppendCeiling
	err := log.Append([]Field{StringField("MESSAGE", "too big")}, testEntryOptions(1))
	if !errors.Is(err, ErrJournalFileFull) || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("Append() error = %v, want ErrJournalFileFull only", err)
	}
	if log.ActivePath() != empty {
		t.Fatal("Log rotated away from an empty file")
	}
}
