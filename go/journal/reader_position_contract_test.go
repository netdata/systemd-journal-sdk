package journal

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// positionCursor adapts the idiomatic readers and the facade to one movement
// contract so a single libsystemd-derived table checks all of them.
type positionCursor interface {
	step() (bool, error)
	stepBack() (bool, error)
	seekHead() error
	seekTail() error
	realtime() (uint64, error)
	addMatch(string)
	refresh() error
	close() error
}

type readerCursor struct{ r *Reader }

func (c readerCursor) step() (bool, error)       { return c.r.Step() }
func (c readerCursor) stepBack() (bool, error)   { return c.r.StepBack() }
func (c readerCursor) seekHead() error           { return c.r.SeekHead() }
func (c readerCursor) seekTail() error           { return c.r.SeekTail() }
func (c readerCursor) realtime() (uint64, error) { return c.r.GetRealtimeUsec() }
func (c readerCursor) addMatch(m string)         { c.r.AddMatch([]byte(m)) }
func (c readerCursor) refresh() error            { _, err := c.r.Refresh(); return err }
func (c readerCursor) close() error              { return c.r.Close() }

type directoryCursor struct{ r *DirectoryReader }

func (c directoryCursor) step() (bool, error)       { return c.r.Step() }
func (c directoryCursor) stepBack() (bool, error)   { return c.r.StepBack() }
func (c directoryCursor) seekHead() error           { return c.r.SeekHead() }
func (c directoryCursor) seekTail() error           { return c.r.SeekTail() }
func (c directoryCursor) realtime() (uint64, error) { return c.r.GetRealtimeUsec() }
func (c directoryCursor) addMatch(m string)         { c.r.AddMatch([]byte(m)) }
func (c directoryCursor) refresh() error            { return nil }
func (c directoryCursor) close() error              { return c.r.Close() }

type facadeCursor struct{ j *sdJournal }

func facadeMove(n int, err error) (bool, error) { return n > 0, err }

func (c facadeCursor) step() (bool, error)       { return facadeMove(SdJournalNext(c.j)) }
func (c facadeCursor) stepBack() (bool, error)   { return facadeMove(SdJournalPrevious(c.j)) }
func (c facadeCursor) seekHead() error           { return SdJournalSeekHead(c.j) }
func (c facadeCursor) seekTail() error           { return SdJournalSeekTail(c.j) }
func (c facadeCursor) realtime() (uint64, error) { return SdJournalGetRealtimeUsec(c.j) }
func (c facadeCursor) addMatch(m string)         { _ = SdJournalAddMatch(c.j, []byte(m)) }
func (c facadeCursor) refresh() error            { return nil }
func (c facadeCursor) close() error              { return SdJournalClose(c.j) }

func positionAppend(t *testing.T, w *Writer, usec uint64, target bool) {
	t.Helper()
	fields := []Field{StringField("MESSAGE", fmt.Sprint("row", usec)), StringField("K", "v")}
	if target {
		fields = append(fields, StringField("TARGET", "new"))
	}
	if err := w.Append(fields, EntryOptions{RealtimeUsec: usec, MonotonicUsec: usec}); err != nil {
		t.Fatal(err)
	}
	if err := w.Sync(); err != nil {
		t.Fatal(err)
	}
}

func positionResult(t *testing.T, c positionCursor, ok bool, err error) uint64 {
	t.Helper()
	if err != nil {
		t.Fatalf("move=%v", err)
	}
	if !ok {
		return 0
	}
	got, err := c.realtime()
	if err != nil {
		t.Fatalf("realtime=%v", err)
	}
	return got
}

// TestPositionContractMatchesLibsystemd encodes the stock libsystemd results
// for next/previous after one live append (SOW-0045). 0 means no entry.
func TestPositionContractMatchesLibsystemd(t *testing.T) {
	// moves checks each setup movement so a failed setup cannot pass silently.
	moves := func(c positionCursor, steps ...bool) error {
		for _, forward := range steps {
			move := c.stepBack
			if forward {
				move = c.step
			}
			if _, err := move(); err != nil {
				return err
			}
		}
		return nil
	}
	states := []struct {
		name           string
		initial        int
		setup          func(positionCursor) error
		next, previous uint64
	}{
		{"empty-open", 0, func(positionCursor) error { return nil }, 1, 0},
		{"seek-head", 1, func(c positionCursor) error { return c.seekHead() }, 1, 0},
		{"on-last-row", 1, func(c positionCursor) error { return moves(c, true) }, 2, 0},
		{"forward-eof", 1, func(c positionCursor) error { return moves(c, true, true) }, 2, 0},
		{"explicit-tail", 1, func(c positionCursor) error { return c.seekTail() }, 0, 2},
		{"backward-eof", 1, func(c positionCursor) error { return moves(c, true, false) }, 2, 0},
	}
	type opener struct {
		name string
		open func(t *testing.T, path string, mode ReaderAccessMode) positionCursor
	}
	openers := []opener{
		{"reader", func(t *testing.T, path string, mode ReaderAccessMode) positionCursor {
			r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
			if err != nil {
				t.Fatal(err)
			}
			return readerCursor{r}
		}},
		{"directory", func(t *testing.T, path string, mode ReaderAccessMode) positionCursor {
			r, err := OpenDirectoryWithOptions(filepath.Dir(path), DefaultReaderOptions().WithAccessMode(mode))
			if err != nil {
				t.Fatal(err)
			}
			return directoryCursor{r}
		}},
		{"facade", func(t *testing.T, path string, mode ReaderAccessMode) positionCursor {
			j, err := SdJournalOpenFileWithOptions(path, 0, DefaultReaderOptions().WithAccessMode(mode))
			if err != nil {
				t.Fatal(err)
			}
			return facadeCursor{j}
		}},
	}
	for _, o := range openers {
		for _, compact := range []bool{false, true} {
			for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
				for _, match := range []string{"", "K=v"} {
					for _, refresh := range []bool{false, true} {
						for _, s := range states {
							for _, backward := range []bool{false, true} {
								name := fmt.Sprintf("%s/compact=%v/%d/match=%q/refresh=%v/%s/backward=%v", o.name, compact, mode, match, refresh, s.name, backward)
								t.Run(name, func(t *testing.T) {
									path := filepath.Join(t.TempDir(), "system.journal")
									opts := testOptions()
									opts.Compact = compact
									w, err := Create(path, opts)
									if err != nil {
										t.Fatal(err)
									}
									defer w.Close()
									for i := 1; i <= s.initial; i++ {
										positionAppend(t, w, uint64(i), false)
									}
									c := o.open(t, path, mode)
									defer c.close()
									if match != "" {
										c.addMatch(match)
									}
									if err := s.setup(c); err != nil {
										t.Fatalf("setup: %v", err)
									}
									positionAppend(t, w, uint64(s.initial+1), false)
									if refresh {
										if err := c.refresh(); err != nil {
											t.Fatal(err)
										}
									}
									var got uint64
									want := s.next
									if backward {
										ok, err := c.stepBack()
										got, want = positionResult(t, c, ok, err), s.previous
									} else {
										ok, err := c.step()
										got = positionResult(t, c, ok, err)
									}
									if got != want {
										t.Fatalf("got row %d, libsystemd returns row %d", got, want)
									}
								})
							}
						}
					}
				}
			}
		}
	}
}

// A match whose DATA does not exist at install time is resolved again on
// movement, as libsystemd looks DATA up by hash every time.
func TestPositionContractAbsentMatchObservesAppend(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("%v/%d", compact, mode), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "system.journal")
				opts := testOptions()
				opts.Compact = compact
				w, err := Create(path, opts)
				if err != nil {
					t.Fatal(err)
				}
				defer w.Close()
				positionAppend(t, w, 1, false)
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				r.AddMatch([]byte("TARGET=new"))
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("absent=%v/%v", ok, err)
				}
				positionAppend(t, w, 2, true)
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("appended=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 2 {
					t.Fatalf("appended time=%d/%v", got, err)
				}
			})
		}
	}
}

// After a failed move, getters keep the last returned entry; libsystemd's
// real_journal_next returns 0 without clearing the current file.
func TestPositionContractGettersAfterFailedMove(t *testing.T) {
	for _, match := range []string{"", "K=v"} {
		t.Run(fmt.Sprintf("match=%q", match), func(t *testing.T) {
			w, r, _ := positionReader(t, 2)
			_ = w
			if match != "" {
				r.AddMatch([]byte(match))
			}
			for range 2 {
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("step=%v/%v", ok, err)
				}
			}
			if ok, err := r.Step(); err != nil || ok {
				t.Fatalf("EOF=%v/%v", ok, err)
			}
			if got, err := r.GetRealtimeUsec(); err != nil || got != 2 {
				t.Fatalf("after EOF=%d/%v", got, err)
			}
			if err := r.SeekTail(); err != nil {
				t.Fatal(err)
			}
			if _, err := r.GetRealtimeUsec(); err == nil {
				t.Fatal("tail location must have no current entry")
			}
		})
	}
}

func positionReader(t *testing.T, rows int) (*Writer, *Reader, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "system.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = w.Close() })
	for i := 1; i <= rows; i++ {
		positionAppend(t, w, uint64(i), false)
	}
	r, err := OpenFile(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = r.Close() })
	return w, r, path
}

// Decision 3: a directory seek has no current entry until a successful move.
func TestDirectorySeekRealtimeClearsCurrentEntry(t *testing.T) {
	_, _, path := positionReader(t, 2)
	dr, err := OpenDirectory(filepath.Dir(path))
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("step=%v/%v", ok, err)
	}
	if err := dr.SeekRealtimeUsec(2); err != nil {
		t.Fatal(err)
	}
	if got, err := dr.GetRealtimeUsec(); err == nil {
		t.Fatalf("seek location reported entry %d", got)
	}
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("step after seek=%v/%v", ok, err)
	}
	if got, err := dr.GetRealtimeUsec(); err != nil || got != 2 {
		t.Fatalf("after seek=%d/%v", got, err)
	}
}

// Decision 4: ENTRY read failures during realtime search are returned, and the
// seek location survives so a retry after repair succeeds.
func TestRealtimeSearchReturnsEntryReadErrors(t *testing.T) {
	for _, backward := range []bool{false, true} {
		t.Run(fmt.Sprintf("backward=%v", backward), func(t *testing.T) {
			_, r, path := positionReader(t, 3)
			middle := r.entryOffsets[1]
			original := make([]byte, 16)
			file, err := os.OpenFile(path, os.O_RDWR, 0)
			if err != nil {
				t.Fatal(err)
			}
			defer file.Close()
			if _, err := file.ReadAt(original, int64(middle)); err != nil {
				t.Fatal(err)
			}
			if _, err := file.WriteAt(make([]byte, 16), int64(middle)); err != nil {
				t.Fatal(err)
			}
			if err := r.SeekRealtimeUsec(2); err != nil {
				t.Fatal(err)
			}
			move := r.Next
			if backward {
				move = r.Previous
			}
			if err := move(); err == nil || errors.Is(err, errEndOfEntries) || errors.Is(err, errStartOfEntries) {
				t.Fatalf("damaged search=%v", err)
			}
			if _, err := file.WriteAt(original, int64(middle)); err != nil {
				t.Fatal(err)
			}
			r.accessor.invalidateCopiedWindows()
			if err := move(); err != nil {
				t.Fatalf("retry=%v", err)
			}
			if got, err := r.GetRealtimeUsec(); err != nil || got != 2 {
				t.Fatalf("retry row=%d/%v", got, err)
			}
		})
	}
}

// Interleaved files force the merged directory path; the boundary and EOF
// states must behave as on a single file.
func TestPositionContractMergedDirectory(t *testing.T) {
	dir := t.TempDir()
	first := testOptions()
	second := testOptions()
	second.SeqnumID[0]++
	second.FileID[0]++
	wa, err := Create(filepath.Join(dir, "a.journal"), first)
	if err != nil {
		t.Fatal(err)
	}
	defer wa.Close()
	wb, err := Create(filepath.Join(dir, "b.journal"), second)
	if err != nil {
		t.Fatal(err)
	}
	defer wb.Close()
	positionAppend(t, wa, 1, false)
	positionAppend(t, wb, 2, false)
	positionAppend(t, wa, 3, false)
	dr, err := OpenDirectory(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	c := directoryCursor{dr}
	expect := func(label string, ok bool, err error, want uint64) {
		t.Helper()
		if got := positionResult(t, c, ok, err); got != want {
			t.Fatalf("%s: got row %d, want %d", label, got, want)
		}
	}
	ok, err := c.stepBack()
	expect("previous at open", ok, err, 0)
	for _, want := range []uint64{1, 2, 3} {
		ok, err = c.step()
		expect("forward", ok, err, want)
	}
	ok, err = c.step()
	expect("forward EOF", ok, err, 0)
	if got, err := c.realtime(); err != nil || got != 3 {
		t.Fatalf("getter after forward EOF=%d/%v, libsystemd keeps row 3", got, err)
	}
	ok, err = c.stepBack()
	expect("reverse after EOF", ok, err, 2)
	ok, err = c.step()
	expect("forward again", ok, err, 3)
	ok, err = c.step()
	expect("forward EOF again", ok, err, 0)
	positionAppend(t, wb, 4, false)
	ok, err = c.step()
	expect("next after append", ok, err, 4)
	ok, err = c.stepBack()
	expect("reverse", ok, err, 3)
	if err := c.seekTail(); err != nil {
		t.Fatal(err)
	}
	ok, err = c.step()
	expect("next at tail", ok, err, 0)
	ok, err = c.stepBack()
	expect("previous at tail", ok, err, 4)
	ok, err = c.stepBack()
	expect("previous", ok, err, 3)
	for range 2 {
		_, _ = c.stepBack()
	}
	ok, err = c.stepBack()
	expect("backward EOF", ok, err, 0)
	if got, err := c.realtime(); err != nil || got != 1 {
		t.Fatalf("getter after backward EOF=%d/%v, libsystemd keeps row 1", got, err)
	}
	if err := c.seekHead(); err != nil {
		t.Fatal(err)
	}
	ok, err = c.stepBack()
	expect("previous at head", ok, err, 0)
	ok, err = c.step()
	expect("next at head", ok, err, 1)
}

// Sequential (non-overlapping) directories keep the last entry current after
// a failed move, like single files.
func TestDirectoryGettersAfterFailedMove(t *testing.T) {
	for _, match := range []string{"", "K=v"} {
		t.Run(fmt.Sprintf("match=%q", match), func(t *testing.T) {
			_, _, path := positionReader(t, 2)
			dr, err := OpenDirectory(filepath.Dir(path))
			if err != nil {
				t.Fatal(err)
			}
			defer dr.Close()
			if match != "" {
				dr.AddMatch([]byte(match))
			}
			for range 2 {
				if ok, err := dr.Step(); err != nil || !ok {
					t.Fatalf("step=%v/%v", ok, err)
				}
			}
			if ok, err := dr.Step(); err != nil || ok {
				t.Fatalf("EOF=%v/%v", ok, err)
			}
			if got, err := dr.GetRealtimeUsec(); err != nil || got != 2 {
				t.Fatalf("realtime after EOF=%d/%v", got, err)
			}
			if _, err := dr.GetCursor(); err != nil {
				t.Fatalf("cursor after EOF=%v", err)
			}
			if ok, err := dr.StepBack(); err != nil || !ok {
				t.Fatalf("reverse=%v/%v", ok, err)
			}
			if got, err := dr.GetRealtimeUsec(); err != nil || got != 1 {
				t.Fatalf("reverse row=%d/%v", got, err)
			}
			if err := dr.SeekTail(); err != nil {
				t.Fatal(err)
			}
			if _, err := dr.GetRealtimeUsec(); err == nil {
				t.Fatal("tail has no current entry")
			}
		})
	}
}

// Decision 6: live tail/EOF checks (unfiltered and filtered tail-backward,
// filtered forward EOF) see appends. Mmap readers read the header from one
// mapping made on the first check; ReadAt readers make none.
func TestLiveEntryCheckObservesAppendWithoutMapping(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, filtered := range []bool{false, true} {
			t.Run(fmt.Sprintf("mode=%v/filtered=%v", mode, filtered), func(t *testing.T) {
				w, _, path := positionReader(t, 2)
				opts := DefaultReaderOptions()
				opts.AccessMode = mode
				r, err := OpenFileWithOptions(path, opts)
				if err != nil {
					t.Fatal(err)
				}
				if got := r.AccessStats().SelectedAccessMode; got != mode {
					t.Skipf("access mode %v unavailable, got %v", mode, got)
				}
				if filtered {
					r.AddMatch([]byte("K=v"))
				}
				accessor := r.accessor.(*rollingReaderAccessor)
				tailBack := func(want uint64) {
					t.Helper()
					r.SeekTail()
					if ok, err := r.StepBack(); err != nil || !ok {
						t.Fatalf("tail previous=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != want {
						t.Fatalf("tail previous row=%d/%v want %d", got, err, want)
					}
				}
				tailBack(2)
				if (accessor.headerWindow != nil) != (mode == ReaderAccessMmap) {
					t.Fatalf("header mapping present=%v in mode %v", accessor.headerWindow != nil, mode)
				}
				before := r.AccessStats()
				for range 5 {
					tailBack(2)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("EOF=%v/%v", ok, err)
				}
				after := r.AccessStats()
				if after.MapCount != before.MapCount || after.ReadWindowCount != before.ReadWindowCount {
					t.Fatalf("unchanged checks mapped windows: %+v -> %+v", before, after)
				}
				positionAppend(t, w, 3, false)
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("next after append=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 3 {
					t.Fatalf("appended row=%d/%v", got, err)
				}
				positionAppend(t, w, 4, false)
				tailBack(4)
				if err := r.Close(); err != nil {
					t.Fatal(err)
				}
				if final := r.AccessStats(); final.MapCount != final.UnmapCount {
					t.Fatalf("close left mappings: %+v", final)
				}
			})
		}
	}
}

func TestLiveEdgeMovesAfterCloseReturnErrors(t *testing.T) {
	for _, filtered := range []bool{false, true} {
		t.Run(fmt.Sprintf("filtered=%v", filtered), func(t *testing.T) {
			_, r, _ := positionReader(t, 2)
			if filtered {
				r.AddMatch([]byte("K=v"))
			}
			r.SeekTail()
			if err := r.Close(); err != nil {
				t.Fatal(err)
			}
			if ok, err := r.StepBack(); err == nil || ok {
				t.Fatalf("tail previous after close=%v/%v, want an error", ok, err)
			}
			// Forward from an explicit tail needs no file access; it must
			// still return no entry.
			if ok, _ := r.Step(); ok {
				t.Fatal("next after close returned an entry")
			}
		})
	}
}

type cursorSeekReader interface {
	Step() (bool, error)
	AddMatch([]byte)
	GetCursor() (string, error)
	GetRealtimeUsec() (uint64, error)
	SeekCursorDirection(string, Direction) error
	Close() error
}

// Paired with Rust position_contract.rs cursor_seek_observes_append.
func TestCursorSeekObservesAppend(t *testing.T) {
	openers := map[string]func(string) (cursorSeekReader, error){
		"file":      func(path string) (cursorSeekReader, error) { return OpenFile(path) },
		"directory": func(path string) (cursorSeekReader, error) { return OpenDirectory(filepath.Dir(path)) },
	}
	for kind, open := range openers {
		for _, filtered := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/filtered=%v", kind, filtered), func(t *testing.T) {
				w, first, path := positionReader(t, 1)
				if ok, err := first.Step(); err != nil || !ok {
					t.Fatalf("first row=%v/%v", ok, err)
				}
				cursor, err := first.GetCursor()
				if err != nil {
					t.Fatal(err)
				}
				r, err := open(path)
				if err != nil {
					t.Fatal(err)
				}
				t.Cleanup(func() { _ = r.Close() })
				if filtered {
					r.AddMatch([]byte("TARGET=new"))
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("absent match=%v/%v", ok, err)
					}
				} else if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("first row=%v/%v", ok, err)
				}
				positionAppend(t, w, 2, true)
				if err := r.SeekCursorDirection(cursor, DirectionForward); err != nil {
					t.Fatal(err)
				}
				want := uint64(1)
				if filtered {
					want = 2
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != want {
					t.Fatalf("seek row=%d/%v want %d", got, err, want)
				}
				if !filtered {
					if ok, err := r.Step(); err != nil || !ok {
						t.Fatalf("next after seek=%v/%v", ok, err)
					}
					if got, _ := r.GetRealtimeUsec(); got != 2 {
						t.Fatalf("next row=%d want 2", got)
					}
				}
			})
		}
	}
}
