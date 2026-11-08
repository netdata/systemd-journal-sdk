package journal

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

func TestWriterArchiveFailureBlocksMutation(t *testing.T) {
	path := filepath.Join(t.TempDir(), "active.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	if err := w.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	synthetic := errors.New("archive file sync failed")
	old := syncArchiveJournalFile
	syncArchiveJournalFile = func(*Writer) error { return synthetic }
	err = w.ArchiveTo(filepath.Join(filepath.Dir(path), "archive.journal"))
	syncArchiveJournalFile = old
	if !errors.Is(err, synthetic) {
		t.Fatalf("ArchiveTo = %v", err)
	}
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	operations := []struct {
		name string
		run  func() error
	}{
		{"append", func() error {
			return w.Append([]Field{StringField("MESSAGE", "retry")}, EntryOptions{MonotonicUsec: 2})
		}},
		{"raw", func() error { return w.AppendRaw([][]byte{[]byte("MESSAGE=retry")}, EntryOptions{MonotonicUsec: 2}) }},
		{"sync", w.Sync},
		{"archive", func() error { return w.ArchiveTo(path) }},
		{"offline", w.CloseOffline},
	}
	for _, operation := range operations {
		if err := operation.run(); !errors.Is(err, ErrWriterFailed) || !errors.Is(err, synthetic) {
			t.Errorf("%s = %v, want failed writer and original cause", operation.name, err)
		}
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(before, after) {
		t.Error("failed writer changed journal bytes")
	}
	if !w.closed || w.arena != nil {
		t.Error("failed close did not release writer resources")
	}
}

func TestWriterArchiveRenameFailureDoesNotRestoreHeader(t *testing.T) {
	path := filepath.Join(t.TempDir(), "active.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	if err := w.ArchiveTo(filepath.Join(path, "missing", "archive.journal")); err == nil {
		t.Fatal("rename succeeded")
	}
	header, err := readJournalHeader(path)
	if err != nil {
		t.Fatal(err)
	}
	if header.state != stateArchived {
		t.Fatalf("state = %d, want archived without rollback", header.state)
	}
}

func TestLogFailedFirstAppendPreservesFile(t *testing.T) {
	l, _ := newTestLog(t, LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: true, OpenMode: LogOpenEager})
	path := l.ActivePath()
	// A small DATA mutates the journal before a large DATA needs an arena remap,
	// whose truncate fails on the deliberately closed descriptor.
	if err := l.writer.file.Close(); err != nil {
		t.Fatal(err)
	}
	err := l.Append([]Field{StringField("MESSAGE", "partial"), {Name: "LARGE", Value: bytes.Repeat([]byte("x"), 16<<20)}}, EntryOptions{MonotonicUsec: 1})
	if err == nil {
		t.Fatal("append with closed descriptor succeeded")
	}
	if l.writer.header.nEntries != 0 {
		t.Fatal("fixture must fail before the first entry commits")
	}
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := l.Close(); err == nil {
		t.Error("close after failed append succeeded")
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed first append file not preserved: %v", err)
	}
	if !bytes.Equal(before, after) {
		t.Error("cleanup rewrote failed journal")
	}
}

func TestLogFailedEmptyClosePreservesFile(t *testing.T) {
	l, _ := newTestLog(t, LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: true, OpenMode: LogOpenEager})
	path := l.ActivePath()
	if err := l.writer.file.Close(); err != nil {
		t.Fatal(err)
	}
	if err := l.Close(); err == nil {
		t.Fatal("close with closed descriptor succeeded")
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("failed empty close removed journal: %v", err)
	}
}

func TestWriterValidationFailureIsReusable(t *testing.T) {
	w, err := Create(filepath.Join(t.TempDir(), "active.journal"), testOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	for _, appendInvalid := range []func() error{
		func() error { return w.Append([]Field{StringField("MESSAGE", "missing clock")}, EntryOptions{}) },
		func() error { return w.AppendRaw([][]byte{[]byte("invalid")}, EntryOptions{MonotonicUsec: 1}) },
		func() error { return w.Append(nil, EntryOptions{MonotonicUsec: 1}) },
	} {
		if err := appendInvalid(); err == nil || errors.Is(err, ErrWriterFailed) {
			t.Fatalf("validation failure = %v", err)
		}
	}
	if err := w.Append([]Field{StringField("MESSAGE", "valid")}, EntryOptions{MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	if err := w.CloseOffline(); err != nil {
		t.Fatal(err)
	}
}

func TestWriterSyncFailureBlocksMutation(t *testing.T) {
	path := filepath.Join(t.TempDir(), "active.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	if err := w.file.Close(); err != nil {
		t.Fatal(err)
	}
	if err := w.Sync(); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("Sync = %v", err)
	}
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "retry")}, EntryOptions{MonotonicUsec: 1}); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("Append = %v", err)
	}
	if err := w.CloseOffline(); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("CloseOffline = %v", err)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(before, after) {
		t.Fatal("mutation after sync failure")
	}
}

func TestLogCreateFailureBlocksTruncatingRetry(t *testing.T) {
	old := newWriterArena
	defer func() { newWriterArena = old }()
	newWriterArena = func(*os.File, uint64) (*mappedArena, error) {
		return nil, errors.New("arena mapping failure")
	}
	l, _ := newTestLog(t, LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: true})
	if err := l.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{MonotonicUsec: 1}); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("first Append = %v", err)
	}
	path := l.ActivePath()
	// A marker makes any second truncation observable, including when the
	// interrupted initializer left a zero-length file.
	marker := []byte("preserve uncertain initialization")
	if err := os.WriteFile(path, marker, 0600); err != nil {
		t.Fatal(err)
	}
	if err := l.Append([]Field{StringField("MESSAGE", "retry")}, EntryOptions{MonotonicUsec: 2}); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("retry Append = %v", err)
	}
	if err := l.Close(); !errors.Is(err, ErrWriterFailed) {
		t.Fatalf("Close = %v", err)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(marker, after) {
		t.Fatal("retry truncated uncertain initialization")
	}
}

func TestLogArchiveFailureCloseReleasesResources(t *testing.T) {
	l, _ := newTestLog(t, LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: true})
	if err := l.Append([]Field{StringField("MESSAGE", "first")}, EntryOptions{MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	w := l.writer
	path := l.ActivePath()
	old := syncArchiveJournalFile
	synthetic := errors.New("archive sync failure")
	syncArchiveJournalFile = func(*Writer) error { return synthetic }
	err := l.Close()
	syncArchiveJournalFile = old
	if !errors.Is(err, synthetic) {
		t.Fatalf("Close = %v", err)
	}
	if !w.closed || w.arena != nil || l.writer != nil {
		t.Fatal("Close failed to release handles")
	}
	if _, err := w.file.Stat(); !errors.Is(err, os.ErrClosed) {
		t.Fatalf("descriptor not closed: %v", err)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("uncertain journal removed: %v", err)
	}
}

func TestLogPoisonedCleanupSkipsRetention(t *testing.T) {
	l, _ := newTestLog(t, LogConfig{
		Options: testOptions(), Source: "system", StrictSystemdNaming: true,
		RetentionPolicy: RetentionPolicy{}.WithMaxFiles(1),
		RotationPolicy:  RotationPolicy{}.WithMaxEntries(1),
	})
	if err := l.Append([]Field{StringField("MESSAGE", "active")}, EntryOptions{RealtimeUsec: 200, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	// Introduce an older archive after open-time retention so normal Close
	// would delete it under MaxFiles=1.
	opts := testOptions()
	archivePath := l.chainPathFor(opts.SeqnumID, 1, 100)
	archived, err := Create(archivePath, opts)
	if err != nil {
		t.Fatal(err)
	}
	if err := archived.Append([]Field{StringField("MESSAGE", "archive")}, EntryOptions{RealtimeUsec: 100, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	if err := archived.ArchiveTo(archivePath); err != nil {
		t.Fatal(err)
	}
	activePath := l.ActivePath()
	old := syncArchiveJournalFile
	synthetic := errors.New("rotation sync failure")
	syncArchiveJournalFile = func(*Writer) error { return synthetic }
	err = l.Append([]Field{StringField("MESSAGE", "rotate")}, EntryOptions{RealtimeUsec: 300, MonotonicUsec: 2})
	syncArchiveJournalFile = old
	if !errors.Is(err, synthetic) {
		t.Fatalf("rotation = %v", err)
	}
	before, err := os.ReadFile(activePath)
	if err != nil {
		t.Fatal(err)
	}
	if err := l.Close(); !errors.Is(err, synthetic) {
		t.Fatalf("Close = %v", err)
	}
	if _, err := os.Stat(archivePath); err != nil {
		t.Fatalf("cleanup applied retention: %v", err)
	}
	after, err := os.ReadFile(activePath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(before, after) {
		t.Fatal("cleanup changed uncertain active")
	}
}

func TestWriterCompactPreflightFailureIsReusable(t *testing.T) {
	for _, raw := range []bool{false, true} {
		for _, sealed := range []bool{false, true} {
			t.Run(fmt.Sprintf("raw=%v/sealed=%v", raw, sealed), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "compact.journal")
				opts := testOptions()
				opts.Compact = true
				if sealed {
					opts.Seal = testSealOpts()
				}
				w, err := Create(path, opts)
				if err != nil {
					t.Fatal(err)
				}
				defer w.Close()
				appendEntry := func(value string, clock uint64) error {
					opts := EntryOptions{RealtimeUsec: 1500000, MonotonicUsec: clock}
					if raw {
						return w.AppendRaw([][]byte{[]byte("MESSAGE=" + value)}, opts)
					}
					return w.Append([]Field{StringField("MESSAGE", value)}, opts)
				}
				if err := appendEntry("existing", 1); err != nil {
					t.Fatal(err)
				}
				before, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				// Model exhausted compact offsets without a multi-gigabyte fixture. All
				// referenced DATA already exists, so rejection precedes any object write.
				offset := w.appendOffset
				w.appendOffset = journalCompactSizeMax - 32
				err = appendEntry("existing", 2)
				w.appendOffset = offset
				if err == nil || errors.Is(err, ErrWriterFailed) {
					t.Fatalf("preflight = %v", err)
				}
				after, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				if !bytes.Equal(before, after) {
					t.Fatal("preflight changed journal bytes")
				}
				if err := appendEntry("existing", 2); err != nil {
					t.Fatalf("retry = %v", err)
				}
				if err := w.CloseOffline(); err != nil {
					t.Fatal(err)
				}
				if err := VerifyIndex(context.Background(), path); err != nil {
					t.Fatal(err)
				}
				if sealed {
					if err := VerifyFileWithKey(path, testVerificationKey(opts.Seal)); err != nil {
						t.Fatal(err)
					}
				}
			})
		}
	}
}

func TestWriterInitialHashTableCapacity(t *testing.T) {
	for _, fieldTable := range []bool{false, true} {
		for _, negative := range []bool{false, true} {
			t.Run(fmt.Sprintf("field=%v/negative=%v", fieldTable, negative), func(t *testing.T) {
				opts := testOptions()
				opts.Compact = true
				buckets := -1
				if !negative {
					// Both cases overflow multiplication by hashItemSize in a native int.
					shift := uint(strconv.IntSize - 4)
					buckets = int(1) << shift
				}
				if fieldTable {
					opts.FieldHashTableBuckets = buckets
				} else {
					opts.DataHashTableBuckets = buckets
				}
				w, err := Create(filepath.Join(t.TempDir(), "oversized.journal"), opts)
				if w != nil {
					defer w.Close()
				}
				if err == nil {
					t.Fatal("invalid hash table capacity accepted")
				}
			})
		}
	}
}

func TestWriterSealingBoundaryRefusedBeforeMutation(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sealed.journal")
	opts := testOptionsWithSeal(testSealOpts())
	opts.Compact = true
	w, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	defer w.Close()
	if err := w.Append([]Field{StringField("MESSAGE", "existing")}, EntryOptions{RealtimeUsec: 1500000, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	before, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	// Advancing the epoch would mutate HMAC state, so the growth pre-check,
	// which counts the epoch's TAG, must refuse before it. Model the compact
	// boundary without mapping a multi-gigabyte fixture.
	appendOffset := w.appendOffset
	w.appendOffset = journalCompactSizeMax - 32
	err = w.Append([]Field{StringField("MESSAGE", "existing")}, EntryOptions{RealtimeUsec: 2500000, MonotonicUsec: 2})
	if !errors.Is(err, ErrJournalFileFull) || errors.Is(err, ErrWriterFailed) {
		t.Fatalf("boundary append = %v, want ErrJournalFileFull only", err)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(before, after) {
		t.Fatal("refused append mutated the journal")
	}
	w.appendOffset = appendOffset
	if err := w.Append([]Field{StringField("MESSAGE", "after")}, EntryOptions{RealtimeUsec: 2500000, MonotonicUsec: 2}); err != nil {
		t.Fatalf("append after refusal = %v", err)
	}
}
