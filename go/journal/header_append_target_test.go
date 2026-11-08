package journal

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// Paired with Rust journal-core file_tests.rs append-target tests.

func u64Bytes(v uint64) []byte {
	return binary.LittleEndian.AppendUint64(nil, v)
}

type appendTargetCase struct {
	name  string
	patch func(t *testing.T, path string)
}

func patchHeaderU32(t *testing.T, path string, offset int64, update func(uint32) uint32) {
	t.Helper()
	value := binary.LittleEndian.Uint32(readFileForTest(t, path)[offset : offset+4])
	patchHeader(t, path, offset, binary.LittleEndian.AppendUint32(nil, update(value)))
}

// Files systemd's writer refuses to extend and that earlier SDK releases
// could leave behind.
func appendTargetCases() []appendTargetCase {
	field := func(offset int64, value uint64) func(*testing.T, string) {
		return func(t *testing.T, path string) { patchHeader(t, path, offset, u64Bytes(value)) }
	}
	return []appendTargetCase{
		{"header-size-280", func(t *testing.T, path string) {
			arena := binary.LittleEndian.Uint64(readFileForTest(t, path)[96:104])
			patchHeader(t, path, 88, u64Bytes(280))
			patchHeader(t, path, 96, u64Bytes(arena-8))
		}},
		{"unknown-compatible-flag", func(t *testing.T, path string) {
			patchHeaderU32(t, path, 8, func(v uint32) uint32 { return v | 1<<3 })
		}},
		{"no-tail-entry-boot-id-flag", func(t *testing.T, path string) {
			patchHeaderU32(t, path, 8, func(v uint32) uint32 { return v &^ compatibleTailEntryBootID })
		}},
		{"archived", func(t *testing.T, path string) { patchHeader(t, path, 16, []byte{stateArchived}) }},
		{"zero-head-realtime", field(184, 0)},
		{"head-realtime-limit", field(184, timestampLimitUsec)},
		{"tail-realtime-limit", field(192, timestampLimitUsec)},
		{"tail-monotonic-limit", field(200, timestampLimitUsec)},
		{"null-tail-boot-id", func(t *testing.T, path string) { patchHeader(t, path, 56, make([]byte, 16)) }},
	}
}

func TestAppendOpenRefusesTargetsSystemdWillNotExtend(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, c := range appendTargetCases() {
			t.Run(c.name, func(t *testing.T) {
				path := headerGateJournal(t, compact)
				c.patch(t, path)
				before := readFileForTest(t, path)
				if w, err := Open(path); !errors.Is(err, errUnsupportedJournal) {
					if w != nil {
						_ = w.Close()
					}
					t.Fatalf("compact=%v: Open() error = %v, want errUnsupportedJournal", compact, err)
				}
				if !bytes.Equal(readFileForTest(t, path), before) {
					t.Fatal("rejected append-open mutated the file")
				}
				// The patched 280-byte header is not a real newer layout: its
				// objects still start at 272, which other checks refuse.
				if c.name == "header-size-280" {
					return
				}
				// Readers keep accepting these files.
				r, err := OpenFile(path)
				if err != nil {
					t.Fatalf("compact=%v: OpenFile() error = %v", compact, err)
				}
				_ = r.Close()
				// Archive-only opens keep earlier releases' files retainable.
				a, err := openForArchive(path)
				if err != nil {
					t.Fatalf("compact=%v: openForArchive() error = %v", compact, err)
				}
				if err := a.Close(); err != nil {
					t.Fatalf("compact=%v: archive writer Close() error = %v", compact, err)
				}
			})
		}
	}
}

func TestAppendOpenRefusesRegularFileBeyondFourGiB(t *testing.T) {
	path := headerGateJournal(t, false)
	original := readFileForTest(t, path)
	if err := os.Truncate(path, int64(journalCompactSizeMax)+pageSize); err != nil {
		t.Fatal(err)
	}
	if w, err := Open(path); !errors.Is(err, errUnsupportedJournal) {
		if w != nil {
			_ = w.Close()
		}
		t.Fatalf("Open() error = %v, want errUnsupportedJournal", err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if uint64(info.Size()) != journalCompactSizeMax+pageSize {
		t.Fatalf("rejected append-open resized the file to %d", info.Size())
	}
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

func TestAppendRefusesOutOfRangeTimestampsWithoutMutation(t *testing.T) {
	cases := []struct {
		name                string
		realtime, monotonic uint64
	}{
		{"zero-realtime", 0, 1},
		{"realtime-limit", timestampLimitUsec, 1},
		{"monotonic-limit", 1_700_000_000_000_000, timestampLimitUsec},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			path := headerGateJournal(t, false)
			w, err := Open(path)
			if err != nil {
				t.Fatal(err)
			}
			defer w.Close()
			before := readFileForTest(t, path)
			err = w.Append([]Field{StringField("MESSAGE", "out-of-range")}, EntryOptions{
				RealtimeUsec: c.realtime, RealtimeUsecSet: true,
				MonotonicUsec: c.monotonic, MonotonicUsecSet: true,
				BootID: UUID{0xee},
			})
			if !errors.Is(err, ErrTimestampOutOfRange) || errors.Is(err, ErrWriterFailed) {
				t.Fatalf("Append() error = %v, want ErrTimestampOutOfRange only", err)
			}
			if !bytes.Equal(readFileForTest(t, path), before) {
				t.Fatal("refused append mutated the file")
			}
			// The writer stays usable.
			if err := w.Append([]Field{StringField("MESSAGE", "valid")}, testEntryOptions(5)); err != nil {
				t.Fatalf("Append(valid) after refusal error = %v", err)
			}
		})
	}
}

func TestLogSetsAsideActiveFileItCannotExtend(t *testing.T) {
	for _, strict := range []bool{false, true} {
		for _, c := range appendTargetCases() {
			// Default chain naming already treats an archived file as history.
			if !strict && c.name == "archived" {
				continue
			}
			t.Run(fmt.Sprintf("strict=%v/%s", strict, c.name), func(t *testing.T) {
				root := t.TempDir()
				config := LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: strict, RotationPolicy: RotationPolicy{}.WithMaxEntries(10)}
				first := mustNewLogForTest(t, root, config, "first")
				appendLogRange(t, first, "set-aside", "", 0, 2, 1_700_002_274_000_000)
				syncLogForTest(t, first, "first")
				activePath := forceCloseActiveWriter(t, first, "first")
				c.patch(t, activePath)

				second := mustNewLogForTest(t, root, config, "second")
				assertDisposedJournalCount(t, second.JournalDirectory(), 1)
				// An out-of-range tail is not a clock floor for the new file.
				const realtime = 1_700_002_274_000_002
				appendLogEntry(t, second, "set-aside-2", "", realtime, 3)
				assertJournalSeqnumRange(t, second.ActivePath(), "replacement", 3, 3)
				if got := readJournalSnapshot(t, second.ActivePath()).header.tailEntryRealtime; got != realtime {
					t.Fatalf("replacement tail realtime = %d, want %d", got, realtime)
				}
				closeLogForTest(t, second, "second")
			})
		}
	}
}

// Root inventory refuses disposed files, so a root Log archives an active file
// it cannot extend instead. Covers the targets earlier releases could write.
func TestRootLogArchivesActiveFileItCannotExtend(t *testing.T) {
	cases := []appendTargetCase{{"beyond-4gib", func(t *testing.T, path string) {
		if err := os.Truncate(path, int64(journalCompactSizeMax)+pageSize); err != nil {
			t.Fatal(err)
		}
	}}}
	for _, c := range appendTargetCases() {
		switch c.name {
		// Root inventory already refused head realtime above tail realtime.
		case "archived", "zero-head-realtime", "tail-realtime-limit", "tail-monotonic-limit", "null-tail-boot-id":
			cases = append(cases, c)
		}
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			root := t.TempDir()
			config := rootTestConfig()
			first := mustNewLogForTest(t, root, config, "first")
			appendLogRange(t, first, "root-set-aside", "", 0, 2, 1_700_002_274_000_000)
			syncLogForTest(t, first, "first")
			activePath := forceCloseActiveWriter(t, first, "first")
			c.patch(t, activePath)
			header := readJournalSnapshot(t, activePath).header
			archivePath := first.archivePathFor(header)

			second := mustNewLogForTest(t, root, config, "second")
			assertDisposedJournalCount(t, second.JournalDirectory(), 0)
			if _, err := os.Stat(activePath); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("active path still exists: %v", err)
			}
			archived := readJournalSnapshot(t, archivePath).header
			if archived.state != stateArchived || archived.nEntries != 2 {
				t.Fatalf("archive state=%d entries=%d, want archived with 2", archived.state, archived.nEntries)
			}
			const realtime = 1_700_002_274_000_002
			appendLogEntry(t, second, "root-set-aside-2", "", realtime, 3)
			assertJournalSeqnumRange(t, second.ActivePath(), "replacement", 3, 3)
			closeLogForTest(t, second, "second")
		})
	}
}

// Paired with Rust append_open_refuses_sealed_file and
// log_sets_aside_sealed_active_file.
func TestAppendOpenRefusesSealedFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sealed.journal")
	w, err := Create(path, testOptionsWithSeal(testSealOpts()))
	if err != nil {
		t.Fatal(err)
	}
	if err := w.Append([]Field{StringField("MESSAGE", "sealed")}, EntryOptions{RealtimeUsec: 1_500_000, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	before := readFileForTest(t, path)
	if w, err := Open(path); !errors.Is(err, ErrUnsupportedJournal) {
		if w != nil {
			_ = w.Close()
		}
		t.Fatalf("Open() error = %v, want ErrUnsupportedJournal", err)
	}
	if !bytes.Equal(readFileForTest(t, path), before) {
		t.Fatal("rejected append-open mutated the file")
	}
	r, err := OpenFile(path)
	if err != nil {
		t.Fatalf("OpenFile() error = %v", err)
	}
	_ = r.Close()
}

func TestLogSetsAsideSealedActiveFile(t *testing.T) {
	// Start sealing near the entries, so the writer seals few epochs.
	seal := &SealOptions{Seed: make([]byte, 12), IntervalUsec: 1_000_000, StartUsec: 1_700_002_273_000_000}
	for _, rootMode := range []bool{false, true} {
		t.Run(fmt.Sprintf("root=%v", rootMode), func(t *testing.T) {
			root := t.TempDir()
			config := LogConfig{Options: testOptionsWithSeal(seal), Source: "system", StrictSystemdNaming: true}
			if rootMode {
				config = rootTestConfig()
				config.Options.Seal = seal
			}
			first := mustNewLogForTest(t, root, config, "first")
			appendLogRange(t, first, "sealed", "", 0, 2, 1_700_002_274_000_000)
			syncLogForTest(t, first, "first")
			activePath := forceCloseActiveWriter(t, first, "first")

			second := mustNewLogForTest(t, root, config, "second")
			want := 1
			if rootMode {
				want = 0
			}
			assertDisposedJournalCount(t, second.JournalDirectory(), want)
			appendLogEntry(t, second, "sealed-2", "", 1_700_002_274_000_002, 3)
			if second.ActivePath() != activePath {
				t.Fatalf("active path = %q, want %q", second.ActivePath(), activePath)
			}
			header, err := readJournalHeader(activePath)
			if err != nil {
				t.Fatal(err)
			}
			if header.headEntrySeqnum != 3 || header.compatibleFlags&compatibleSealed == 0 {
				t.Fatalf("replacement head=%d flags=%#x, want a sealed file from seqnum 3", header.headEntrySeqnum, header.compatibleFlags)
			}
			closeLogForTest(t, second, "second")
		})
	}
}

func TestReaderOpenRefusesUnknownStateAndShortSealedHeader(t *testing.T) {
	t.Run("state-3", func(t *testing.T) {
		path := headerGateJournal(t, false)
		patchHeader(t, path, 16, []byte{3})
		if r, err := OpenFile(path); !errors.Is(err, errInvalidJournal) {
			if r != nil {
				_ = r.Close()
			}
			t.Fatalf("OpenFile() error = %v, want errInvalidJournal", err)
		}
	})
	t.Run("sealed-header-232", func(t *testing.T) {
		path := headerGateJournal(t, false)
		patchHeaderU32(t, path, 8, func(v uint32) uint32 { return v | compatibleSealed })
		arena := binary.LittleEndian.Uint64(readFileForTest(t, path)[96:104])
		patchHeader(t, path, 88, u64Bytes(232))
		patchHeader(t, path, 96, u64Bytes(arena+40))
		if r, err := OpenFile(path); !errors.Is(err, errInvalidJournal) {
			if r != nil {
				_ = r.Close()
			}
			t.Fatalf("OpenFile() error = %v, want errInvalidJournal", err)
		}
	})
}

func everyOpenRefusalCases() []appendTargetCase {
	return []appendTargetCase{
		{"state-3", func(t *testing.T, path string) { patchHeader(t, path, 16, []byte{3}) }},
		{"sealed-header-232", func(t *testing.T, path string) {
			patchHeaderU32(t, path, 8, func(v uint32) uint32 { return v | compatibleSealed })
			arena := binary.LittleEndian.Uint64(readFileForTest(t, path)[96:104])
			patchHeader(t, path, 88, u64Bytes(232))
			patchHeader(t, path, 96, u64Bytes(arena+40))
		}},
	}
}

// journald sets these aside through -EBADMSG and continues the sequence.
// Paired with Rust log_sets_aside_active_file_with_refused_header.
func TestLogSetsAsideActiveFileWithRefusedHeader(t *testing.T) {
	for _, strict := range []bool{false, true} {
		for _, c := range everyOpenRefusalCases() {
			t.Run(fmt.Sprintf("strict=%v/%s", strict, c.name), func(t *testing.T) {
				root := t.TempDir()
				config := LogConfig{Options: testOptions(), Source: "system", StrictSystemdNaming: strict, RotationPolicy: RotationPolicy{}.WithMaxEntries(10)}
				first := mustNewLogForTest(t, root, config, "first")
				appendLogRange(t, first, "refused", "", 0, 2, 1_700_002_274_000_000)
				syncLogForTest(t, first, "first")
				activePath := forceCloseActiveWriter(t, first, "first")
				c.patch(t, activePath)

				second := mustNewLogForTest(t, root, config, "second")
				// Default chain naming only treats online files as active,
				// so state 3 stays behind as history.
				want := 1
				if !strict && c.name == "state-3" {
					want = 0
				}
				assertDisposedJournalCount(t, second.JournalDirectory(), want)
				appendLogEntry(t, second, "refused-2", "", 1_700_002_274_000_002, 3)
				assertJournalSeqnumRange(t, second.ActivePath(), "replacement", 3, 3)
				closeLogForTest(t, second, "second")
			})
		}
	}
}

// A root Log can neither keep a disposed file nor archive a header every open
// refuses, so it reports the error and leaves the file alone.
// Paired with Rust root_log_refuses_active_file_with_refused_header.
func TestRootLogRefusesActiveFileWithRefusedHeader(t *testing.T) {
	for _, c := range everyOpenRefusalCases() {
		t.Run(c.name, func(t *testing.T) {
			root := t.TempDir()
			config := rootTestConfig()
			first := mustNewLogForTest(t, root, config, "first")
			appendLogRange(t, first, "root-refused", "", 0, 2, 1_700_002_274_000_000)
			syncLogForTest(t, first, "first")
			activePath := forceCloseActiveWriter(t, first, "first")
			c.patch(t, activePath)
			before := readFileForTest(t, activePath)

			if log, err := NewLog(root, config); !errors.Is(err, errInvalidJournal) {
				if log != nil {
					_ = log.Close()
				}
				t.Fatalf("NewLog error = %v, want errInvalidJournal", err)
			}
			if !bytes.Equal(readFileForTest(t, activePath), before) {
				t.Fatal("refused root open mutated the file")
			}
			assertDisposedJournalCount(t, filepath.Dir(activePath), 0)
		})
	}
}

// Writable opens apply the same every-open refusals as readers, before any
// mutation. Paired with Rust
// writable_open_refuses_unknown_state_and_short_sealed_header.
func TestWritableOpenRefusesUnknownStateAndShortSealedHeader(t *testing.T) {
	cases := everyOpenRefusalCases()
	opens := []struct {
		name string
		open func(string) (*Writer, error)
	}{
		{"append", func(path string) (*Writer, error) { return Open(path) }},
		{"archive", openForArchive},
	}
	for _, c := range cases {
		for _, o := range opens {
			t.Run(c.name+"/"+o.name, func(t *testing.T) {
				path := headerGateJournal(t, false)
				c.patch(t, path)
				before := readFileForTest(t, path)
				if w, err := o.open(path); !errors.Is(err, errInvalidJournal) {
					if w != nil {
						_ = w.Close()
					}
					t.Fatalf("open error = %v, want errInvalidJournal", err)
				}
				if !bytes.Equal(readFileForTest(t, path), before) {
					t.Fatal("refused writable open mutated the file")
				}
			})
		}
	}
}

// SDK writers before this check, like systemd's, stored a truncated 32-bit
// tail entry array hint once a regular file grew past 4 GiB.
func TestReadersIgnoreTailArrayHintAboveFourGiB(t *testing.T) {
	for _, large := range []bool{false, true} {
		t.Run(fmt.Sprintf("large=%v", large), func(t *testing.T) {
			path := verifyIndexFixture(t, 3, false)
			if large {
				size := int64(journalCompactSizeMax) + 1 + pageSize
				if err := os.Truncate(path, size); err != nil {
					t.Fatal(err)
				}
				patchHeader(t, path, 96, u64Bytes(uint64(size)-headerSize))
			}
			// A truncated hint names an offset below the first entry array.
			patchHeader(t, path, 256, binary.LittleEndian.AppendUint32(nil, 8))

			s, err := OpenIndexedSnapshot(context.Background(), path, IndexedSnapshotOptions{})
			if !large {
				// Below 4 GiB the hint is exact, so a wrong one is corruption.
				if err == nil {
					s.Close()
					t.Fatal("OpenIndexedSnapshot() accepted a wrong hint below 4 GiB")
				}
				if VerifyIndex(context.Background(), path) == nil {
					t.Fatal("VerifyIndex() accepted a wrong hint below 4 GiB")
				}
				return
			}
			if err != nil {
				t.Fatalf("OpenIndexedSnapshot() error = %v", err)
			}
			s.Close()
			r, err := OpenFile(path)
			if err != nil {
				t.Fatalf("OpenFile() error = %v", err)
			}
			rows := 0
			for r.Next() == nil {
				rows++
			}
			_ = r.Close()
			if rows != 3 {
				t.Fatalf("rows = %d, want 3", rows)
			}
			if err := VerifyIndex(context.Background(), path); err != nil {
				t.Fatalf("VerifyIndex() error = %v", err)
			}
		})
	}
}

// Paired with Rust log_refuses_out_of_range_timestamps_without_moving_its_clocks.
// A zero realtime cannot reach the writer: Log raises it to its clock floor.
func TestLogRefusesOutOfRangeTimestampsWithoutMovingItsClocks(t *testing.T) {
	cases := []struct {
		name string
		opts EntryOptions
	}{
		{"realtime-limit", EntryOptions{RealtimeUsec: timestampLimitUsec, RealtimeUsecSet: true, MonotonicUsec: 10, MonotonicUsecSet: true}},
		{"monotonic-limit", EntryOptions{MonotonicUsec: timestampLimitUsec, MonotonicUsecSet: true}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			root := t.TempDir()
			config := LogConfig{Options: testOptions(), Source: "system"}
			log := mustNewLogForTest(t, root, config, "log")
			err := log.Append([]Field{StringField("MESSAGE", "out of range")}, c.opts)
			if !errors.Is(err, ErrTimestampOutOfRange) {
				t.Fatalf("Append() error = %v, want ErrTimestampOutOfRange", err)
			}
			dir := filepath.Join(root, config.Options.MachineID.String())
			assertJournalFileCount(t, dir, "refused before rotation", 0)
			const realtime = 1_700_002_275_000_001
			appendLogEntry(t, log, "valid", "", realtime, 5)
			header := readJournalSnapshot(t, log.ActivePath()).header
			if header.tailEntryRealtime != realtime || header.tailEntryMonotonic != 5 {
				t.Fatalf("tail = %d/%d, want %d/5", header.tailEntryRealtime, header.tailEntryMonotonic, realtime)
			}
			closeLogForTest(t, log, "log")
		})
	}
}
