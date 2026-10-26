package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"syscall"
	"testing"
)

type filterAccessTrace struct {
	readerAccessor
	forbidden map[uint64]bool
	faultAt   uint64
	fail      bool
	calls     int
}

func (a *filterAccessTrace) check(offset, size uint64) error {
	a.calls++
	if a.fail && offset <= a.faultAt && a.faultAt-offset < size {
		return syscall.EIO
	}
	for at := range a.forbidden {
		if offset <= at && at-offset < size {
			return fmt.Errorf("unexpected unrelated DATA read at %d", at)
		}
	}
	return nil
}
func (a *filterAccessTrace) readAt(dst []byte, offset uint64) error {
	if err := a.check(offset, uint64(len(dst))); err != nil {
		return err
	}
	return a.readerAccessor.readAt(dst, offset)
}
func (a *filterAccessTrace) tempSlice(offset, size uint64) ([]byte, error) {
	if err := a.check(offset, size); err != nil {
		return nil, err
	}
	return a.readerAccessor.tempSlice(offset, size)
}

func filterTestReader(t *testing.T, data []byte, mode ReaderAccessMode, live bool) (*Reader, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "system.journal")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(1024).WithMaxWindows(1).WithSnapshot(!live))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if r.accessor != nil {
			if err := r.Close(); err != nil {
				t.Error(err)
			}
		}
	})
	return r, path
}

func filterTestDataOffset(t *testing.T, r *Reader, payload string) uint64 {
	t.Helper()
	offset, found, err := r.findDataOffsetByPayload([]byte(payload))
	if err != nil || !found {
		t.Fatalf("lookup %s: %v/%v", payload, found, err)
	}
	return offset
}

func TestNativeFilterSelectiveAccessAndRestart(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("%v/%d", compact, mode), func(t *testing.T) {
				r, _ := filterTestReader(t, source, mode, false)
				unrelated := filterTestDataOffset(t, r, "MESSAGE=row-0001")
				common := filterTestDataOffset(t, r, "COMMON=all")
				array := binary.LittleEndian.Uint64(source[common+48:])
				trace := &filterAccessTrace{readerAccessor: r.accessor, forbidden: map[uint64]bool{unrelated: true}}
				r.accessor = trace
				r.AddMatch([]byte("SPARSE=some"))
				if got := recoveryTestTimes(t, r, false); !slices.Equal(got, []uint64{1000, 1500, 2004}) {
					t.Fatalf("sparse=%v", got)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 2004 {
					t.Fatalf("EOF position=%d/%v", got, err)
				}
				if err := r.SeekTail(); err != nil {
					t.Fatal(err)
				}
				if got := recoveryTestTimes(t, r, true); !slices.Equal(got, []uint64{2004, 1500, 1000}) {
					t.Fatalf("reverse=%v", got)
				}
				for _, tc := range []struct {
					time uint64
					back bool
					want uint64
				}{{1250, false, 1500}, {1750, true, 1500}, {0, false, 1000}, {9999, true, 2004}} {
					if err := r.SeekRealtimeUsec(tc.time); err != nil {
						t.Fatal(err)
					}
					var ok bool
					var err error
					if tc.back {
						ok, err = r.StepBack()
					} else {
						ok, err = r.Step()
					}
					if err != nil || !ok {
						t.Fatalf("seek=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != tc.want {
						t.Fatalf("time=%d/%v want%d", got, err, tc.want)
					}
				}
				r.FlushMatches()
				r.AddMatch([]byte("COMMON=all"))
				trace.faultAt, trace.fail = array, true
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("first dense step eagerly read array: %v/%v", ok, err)
				}
				if ok, err := r.Step(); ok || !errors.Is(err, syscall.EIO) {
					t.Fatalf("posting error=%v/%v", ok, err)
				}
				trace.fail = false
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("posting retry=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
					t.Fatalf("retry time=%d/%v", got, err)
				}
				r.FlushMatches()
				r.AddMatch([]byte("MISSING=value"))
				for _, offset := range r.entryOffsets {
					trace.forbidden[offset] = true
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("absent=%v/%v", ok, err)
				}
			})
		}
	}
}

func TestNativeFilterBuildRetryAndMalformed(t *testing.T) {
	source, _ := recoveryTestFixture(t, false)
	r, path := filterTestReader(t, source, ReaderAccessReadAt, false)
	common := filterTestDataOffset(t, r, "COMMON=all")
	trace := &filterAccessTrace{readerAccessor: r.accessor, faultAt: common, fail: true}
	r.accessor = trace
	r.AddMatch([]byte("COMMON=all"))
	if err := r.AddConjunction(); !errors.Is(err, syscall.EIO) {
		t.Fatalf("operator error=%v", err)
	}
	trace.fail = false
	if err := r.AddConjunction(); err != nil {
		t.Fatal(err)
	}
	if ok, err := r.Step(); err != nil || !ok {
		t.Fatalf("build retry=%v/%v", ok, err)
	}
	if got, err := r.GetRealtimeUsec(); err != nil || got != 1000 {
		t.Fatalf("first=%d/%v", got, err)
	}
	r.AddMatch([]byte("MESSAGE=row-0500"))
	if ok, err := r.Step(); err != nil || !ok {
		t.Fatalf("replacement=%v/%v", ok, err)
	}
	if got, err := r.GetRealtimeUsec(); err != nil || got != 1500 {
		t.Fatalf("replacement time=%d/%v", got, err)
	}
	for _, op := range []string{"invalid", "and", "or"} {
		r.FlushMatches()
		switch op {
		case "invalid":
			r.AddMatch([]byte("invalid"))
		case "and":
			if err := r.AddConjunction(); err != nil {
				t.Fatal(err)
			}
		case "or":
			if err := r.AddDisjunction(); err != nil {
				t.Fatal(err)
			}
		}
		if ok, err := r.Step(); ok || !errors.Is(err, ErrMalformedFilter) {
			t.Fatalf("%s=%v/%v", op, ok, err)
		}
	}
	r.FlushMatches()
	r.AddMatch([]byte("COMMON=all"))
	file, err := os.OpenFile(path, os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	_, err = file.WriteAt(make([]byte, 16), 104)
	closeErr := file.Close()
	if err != nil || closeErr != nil {
		t.Fatalf("remove hash table=%v/%v", err, closeErr)
	}
	if ok, err := r.Step(); ok || err == nil {
		t.Fatalf("missing hash table=%v/%v", ok, err)
	}
}

func TestNativeFilterRecoveredSparsePositionAndRepair(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		last := arrays[len(arrays)-1].entries[len(arrays[len(arrays)-1].entries)-1]
		damaged := bytes.Clone(source)
		clear(damaged[last : last+16])
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("%v/%d", compact, mode), func(t *testing.T) {
				r, path := filterTestReader(t, damaged, mode, true)
				r.AddMatch([]byte("SPARSE=some"))
				for _, want := range []uint64{1000, 1500} {
					if ok, err := r.Step(); err != nil || !ok {
						t.Fatalf("step=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != want {
						t.Fatalf("time=%d/%v", got, err)
					}
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("EOF=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1500 {
					t.Fatalf("EOF position=%d/%v", got, err)
				}
				if ok, err := r.StepBack(); err != nil || !ok {
					t.Fatalf("reverse=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1000 {
					t.Fatalf("reverse position=%d/%v", got, err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("forward=%v/%v", ok, err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("second EOF=%v/%v", ok, err)
				}
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				_, err = file.WriteAt(source[last:last+16], int64(last))
				closeErr := file.Close()
				if err != nil || closeErr != nil {
					t.Fatalf("repair=%v/%v", err, closeErr)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("unchanged EOF=%v/%v", ok, err)
				}
				if err := r.SeekRealtimeUsec(1501); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("repair step=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 2004 {
					t.Fatalf("repair position=%d/%v", got, err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("repair repeated=%v/%v", ok, err)
				}
				if r.RecoveryStatus().Incomplete() {
					t.Fatal("repair remains degraded")
				}
			})
		}
	}
}

func TestNativeFilterDirectoryCandidateModesAndMutation(t *testing.T) {
	dir := t.TempDir()
	a := filepath.Join(dir, "a.journal")
	b := filepath.Join(dir, "b.journal")
	createMessageJournal(t, a, []messageRow{{"keep", 1000}, {"other", 1002}, {"other", 1004}})
	createMessageJournal(t, b, []messageRow{{"other", 1001}, {"keep", 1003}, {"other", 1005}})
	dr, err := OpenFiles([]string{a, b})
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	dr.AddMatch([]byte("MESSAGE=keep"))
	for _, tc := range []struct {
		filtered bool
		want     uint64
	}{{true, 1000}, {false, 1001}, {true, 1003}, {false, 1004}} {
		var ok bool
		var err error
		if tc.filtered {
			ok, err = dr.Step()
		} else {
			err = dr.Next()
			ok = err == nil
		}
		if err != nil || !ok {
			t.Fatalf("step=%v/%v", ok, err)
		}
		if got, err := dr.GetRealtimeUsec(); err != nil || got != tc.want {
			t.Fatalf("time=%d/%v want%d", got, err, tc.want)
		}
	}
	dr.FlushMatches()
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("flush=%v/%v", ok, err)
	}
	dr.AddMatch([]byte("MESSAGE=keep"))
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("mutation=%v/%v", ok, err)
	}
	if got, err := dr.GetRealtimeUsec(); err != nil || got != 1003 {
		t.Fatalf("mutation time=%d/%v", got, err)
	}
	cursor, err := dr.GetCursor()
	if err != nil {
		t.Fatal(err)
	}
	if ok, err := dr.Step(); err != nil || ok {
		t.Fatalf("end=%v/%v", ok, err)
	}
	if _, err := dr.GetRealtimeUsec(); err == nil {
		t.Fatal("directory EOF retained a current row")
	}
	if ok, err := dr.TestCursor(cursor); !errors.Is(err, ErrEndOfEntries) || ok {
		t.Fatalf("EOF cursor=%v/%v", ok, err)
	}
}

func TestNativeFilterSnapshotRepairAndAfterOpenLoss(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		last := arrays[len(arrays)-1].entries[len(arrays[len(arrays)-1].entries)-1]
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("snapshot/%v/%d", compact, mode), func(t *testing.T) {
				damaged := bytes.Clone(source)
				clear(damaged[last : last+16])
				r, path := filterTestReader(t, damaged, mode, false)
				r.AddMatch([]byte("MESSAGE=row-1004"))
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("missing row=%v/%v", ok, err)
				}
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				_, err = file.WriteAt(source[last:last+16], int64(last))
				closeErr := file.Close()
				if err != nil || closeErr != nil {
					t.Fatalf("repair=%v/%v", err, closeErr)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("unchanged EOF=%v/%v", ok, err)
				}
				if err := r.SeekRealtimeUsec(2004); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("snapshot repaired row=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 2004 {
					t.Fatalf("snapshot time=%d/%v", got, err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("snapshot repeated=%v/%v", ok, err)
				}
			})
		}
		t.Run(fmt.Sprintf("after-open/%v", compact), func(t *testing.T) {
			r, path := filterTestReader(t, source, ReaderAccessMmap, true)
			pair := filterTestDataOffset(t, r, "TAIL_PAIR=some")
			file, err := os.OpenFile(path, os.O_WRONLY, 0)
			if err != nil {
				t.Fatal(err)
			}
			if _, err := file.WriteAt(make([]byte, 16), int64(last)); err != nil {
				t.Fatal(err)
			}
			if _, err := file.WriteAt(make([]byte, 8), int64(pair+48)); err != nil {
				t.Fatal(err)
			}
			if err := file.Close(); err != nil {
				t.Fatal(err)
			}
			r.AddMatch([]byte("TAIL_PAIR=some"))
			r.AddMatch([]byte("COMMON=all"))
			if got := recoveryTestTimes(t, r, false); !slices.Equal(got, []uint64{2003}) {
				t.Fatalf("surviving pair=%v", got)
			}
			if r.RecoveryStatus().RecoveredEntries != 1004 {
				t.Fatalf("status=%+v", r.RecoveryStatus())
			}
		})
	}
}

func TestNativeFilterOrphanAndDuplicatePostingParity(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("orphan/%v/%d", compact, mode), func(t *testing.T) {
				damaged := bytes.Clone(source)
				binary.LittleEndian.PutUint64(damaged[152:], 1)
				r, _ := filterTestReader(t, damaged, mode, false)
				r.AddMatch([]byte("MESSAGE=row-0500"))
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("orphan=%v/%v", ok, err)
				}
				entry, err := r.GetEntry()
				if err != nil || entry.Realtime != 1500 || string(entry.Fields["MESSAGE"]) != "row-0500" {
					t.Fatalf("orphan entry=%v/%v", entry, err)
				}
				cursor, err := r.GetCursor()
				if err != nil {
					t.Fatal(err)
				}
				if err := r.SeekCursor(cursor); err != nil {
					t.Fatal(err)
				}
				// Realtime seeking uses the global array, so the valid orphan is
				// reachable by head/tail exact traversal, not by this time bound.
				if err := r.SeekTail(); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.StepBack(); err != nil || !ok {
					t.Fatalf("reverse orphan=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1500 {
					t.Fatalf("reverse orphan time=%d/%v", got, err)
				}
				if err := r.Previous(); err != nil {
					t.Fatal(err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1000 {
					t.Fatalf("raw before orphan=%d/%v", got, err)
				}
			})
		}
		t.Run(fmt.Sprintf("duplicate/%v", compact), func(t *testing.T) {
			r, _ := filterTestReader(t, source, ReaderAccessMmap, false)
			common := filterTestDataOffset(t, r, "COMMON=all")
			array := binary.LittleEndian.Uint64(source[common+48:])
			damaged := bytes.Clone(source)
			slot := damaged[array+24:]
			if compact {
				binary.LittleEndian.PutUint32(slot, uint32(arrays[0].entries[0]))
			} else {
				binary.LittleEndian.PutUint64(slot, arrays[0].entries[0])
			}
			candidate, _ := filterTestReader(t, damaged, ReaderAccessMmap, false)
			candidate.AddMatch([]byte("COMMON=all"))
			got := recoveryTestTimes(t, candidate, false)
			if len(got) != 1004 || got[0] != 1000 || got[1] != 1002 || got[len(got)-1] != 2004 {
				t.Fatalf("duplicate traversal count=%d", len(got))
			}
		})
	}
}

func TestNativeFilterDirectoryModeSwitchPreservesNonoverlapPosition(t *testing.T) {
	dir := t.TempDir()
	a := filepath.Join(dir, "a.journal")
	b := filepath.Join(dir, "b.journal")
	for _, tc := range []struct {
		path string
		head uint64
		rows []messageRow
	}{
		{a, 1, []messageRow{{"other", 1000}, {"keep", 1001}, {"other", 1002}}},
		{b, 4, []messageRow{{"other", 2000}, {"keep", 2001}, {"other", 2002}}},
	} {
		opts := testOptions()
		opts.HeadSeqnum = tc.head
		w, err := Create(tc.path, opts)
		if err != nil {
			t.Fatal(err)
		}
		for i, row := range tc.rows {
			if err := w.Append([]Field{StringField("MESSAGE", row.message)}, EntryOptions{RealtimeUsec: row.realtime, MonotonicUsec: tc.head + uint64(i)}); err != nil {
				t.Fatal(err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatal(err)
		}
	}
	dr, err := OpenFiles([]string{a, b})
	if err != nil {
		t.Fatal(err)
	}
	defer dr.Close()
	if !dr.nonOverlapping {
		t.Fatal("fixture did not exercise sequential proof")
	}
	dr.AddMatch([]byte("MESSAGE=keep"))
	if ok, err := dr.Step(); err != nil || !ok {
		t.Fatalf("filtered=%v/%v", ok, err)
	}
	for _, want := range []uint64{1002, 2000, 2001, 2002} {
		if err := dr.Next(); err != nil {
			t.Fatal(err)
		}
		if got, err := dr.GetRealtimeUsec(); err != nil || got != want {
			t.Fatalf("raw=%d/%v want%d", got, err, want)
		}
	}
}

func TestNativeFilterDoesNotDecodeUnrelatedCompressedData(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		r, _ := filterTestReader(t, source, ReaderAccessMmap, false)
		message := filterTestDataOffset(t, r, "MESSAGE=row-0000")
		damaged := bytes.Clone(source)
		damaged[message+1] = objectCompressedZSTD
		candidate, _ := filterTestReader(t, damaged, ReaderAccessReadAt, false)
		candidate.AddMatch([]byte("SPARSE=some"))
		if ok, err := candidate.Step(); err != nil || !ok {
			t.Fatalf("metadata movement=%v/%v", ok, err)
		}
		cursor, err := candidate.GetCursor()
		if err != nil {
			t.Fatal(err)
		}
		if err := candidate.SeekCursor(cursor); err != nil {
			t.Fatal(err)
		}
		if got, err := candidate.GetRealtimeUsec(); err != nil || got != 1000 {
			t.Fatalf("cursor time=%d/%v", got, err)
		}
		if _, err := candidate.GetEntry(); err == nil {
			t.Fatal("payload control did not detect malformed compression")
		}
	}
}

func TestNativeFilterRecoveredTailAnchorSurvivesRepair(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, backward := range []bool{false, true} {
			t.Run(fmt.Sprintf("%d/%v", mode, backward), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "source.journal")
				createMessageJournal(t, path, []messageRow{{"only", 1000}})
				source, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				array := binary.LittleEndian.Uint64(source[176:])
				entry := binary.LittleEndian.Uint64(source[array+24:])
				damaged := bytes.Clone(source)
				clear(damaged[entry : entry+16])
				r, target := filterTestReader(t, damaged, mode, false)
				r.AddMatch([]byte("MESSAGE=only"))
				if err := r.SeekTail(); err != nil {
					t.Fatal(err)
				}
				file, err := os.OpenFile(target, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				_, err = file.WriteAt(source[entry:entry+16], int64(entry))
				closeErr := file.Close()
				if err != nil || closeErr != nil {
					t.Fatalf("repair=%v/%v", err, closeErr)
				}
				var ok bool
				if backward {
					ok, err = r.StepBack()
				} else {
					ok, err = r.Step()
				}
				if err != nil || ok != backward {
					t.Fatalf("tail step=%v/%v", ok, err)
				}
				time, timeErr := r.GetRealtimeUsec()
				if backward && (timeErr != nil || time != 1000) {
					t.Fatalf("tail time=%d/%v", time, timeErr)
				}
				if !backward && timeErr == nil {
					t.Fatal("tail became a resolved row after repair")
				}
			})
		}
	}
}

func TestNativeFilterNewGroupsObserveHealthyAppend(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, grow := range []bool{false, true} {
				t.Run(fmt.Sprintf("%v/%d/grow=%v", compact, mode, grow), func(t *testing.T) {
					path := filepath.Join(t.TempDir(), "system.journal")
					opts := testOptions()
					opts.Compact = compact
					w, err := Create(path, opts)
					if err != nil {
						t.Fatal(err)
					}
					defer w.Close()
					if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 1}); err != nil {
						t.Fatal(err)
					}
					r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode).WithWindowSize(1024).WithMaxWindows(1))
					if err != nil {
						t.Fatal(err)
					}
					defer r.Close()
					oldSize := r.fileSize
					r.AddMatch([]byte("TARGET=new"))
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("absent=%v/%v", ok, err)
					}
					fields := []Field{StringField("COMMON", "all"), StringField("TARGET", "new")}
					if grow {
						fields = append([]Field{StringField("LARGE", string(bytes.Repeat([]byte{'x'}, int(oldSize)+4096)))}, fields...)
					}
					if err := w.Append(fields, EntryOptions{RealtimeUsec: 1001, MonotonicUsec: 2}); err != nil {
						t.Fatal(err)
					}
					if grow {
						info, err := os.Stat(path)
						if err != nil || uint64(info.Size()) <= oldSize {
							t.Fatalf("no physical growth: %v", err)
						}
					}
					if ok, err := r.Step(); err != nil || ok {
						t.Fatalf("resolved absent leaf changed=%v/%v", ok, err)
					}
					trace := &filterAccessTrace{readerAccessor: r.accessor, forbidden: map[uint64]bool{r.entryOffsets[0]: true}}
					r.accessor = trace
					r.AddMatch([]byte("TARGET=new"))
					if err := r.AddConjunction(); err != nil {
						t.Fatal(err)
					}
					if ok, err := r.Step(); err != nil || !ok {
						t.Fatalf("new group=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
						t.Fatalf("new group time=%d/%v", got, err)
					}
					trace.forbidden = nil
					if err := r.Previous(); err != nil {
						t.Fatal(err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != 1000 {
						t.Fatalf("raw before new selected row=%d/%v", got, err)
					}
					r.FlushMatches()
					r.AddMatch([]byte("COMMON=all"))
					if got := recoveryTestTimes(t, r, false); !slices.Equal(got, []uint64{1000, 1001}) {
						t.Fatalf("existing DATA=%v", got)
					}
					if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 1002, MonotonicUsec: 3}); err != nil {
						t.Fatal(err)
					}
					if err := r.SeekHead(); err != nil {
						t.Fatal(err)
					}
					if got := recoveryTestTimes(t, r, false); !slices.Equal(got, []uint64{1000, 1001}) {
						t.Fatalf("captured population changed=%v", got)
					}
					r.AddMatch([]byte("COMMON=all"))
					if ok, err := r.Step(); err != nil || !ok {
						t.Fatalf("replacement existing DATA=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != 1002 {
						t.Fatalf("replacement time=%d/%v", got, err)
					}
				})
			}
		}
	}
}

func TestNativeFilterGroupVisibilityKeepsRawGrowth(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, initial := range []int{0, 1} {
				for _, failBuild := range []bool{false, true} {
					for _, clearFilter := range []bool{false, true} {
						for _, refresh := range []bool{false, true} {
							t.Run(fmt.Sprintf("%v/%d/initial=%d/failure=%v/clear=%v/refresh=%v", compact, mode, initial, failBuild, clearFilter, refresh), func(t *testing.T) {
								path := filepath.Join(t.TempDir(), "system.journal")
								opts := testOptions()
								opts.Compact = compact
								w, err := Create(path, opts)
								if err != nil {
									t.Fatal(err)
								}
								defer w.Close()
								appendRow := func(stamp uint64) {
									t.Helper()
									if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: stamp, MonotonicUsec: stamp}); err != nil {
										t.Fatal(err)
									}
								}
								if initial != 0 {
									appendRow(1000)
								}
								r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
								if err != nil {
									t.Fatal(err)
								}
								defer r.Close()
								if err := r.SeekHead(); err != nil {
									t.Fatal(err)
								}
								if initial != 0 {
									if err := r.Next(); err != nil {
										t.Fatal(err)
									}
								}
								appendRow(1001)
								if failBuild {
									probe, err := OpenFile(path)
									if err != nil {
										t.Fatal(err)
									}
									data := filterTestDataOffset(t, probe, "COMMON=all")
									if err := probe.Close(); err != nil {
										t.Fatal(err)
									}
									trace := &filterAccessTrace{readerAccessor: r.accessor, faultAt: data, fail: true}
									r.accessor = trace
									r.AddMatch([]byte("COMMON=all"))
									if err := r.AddDisjunction(); !errors.Is(err, syscall.EIO) {
										t.Fatalf("group failure=%v", err)
									}
									trace.fail = false
								} else {
									r.AddMatch([]byte("MISSING=value"))
									if ok, err := r.Step(); err != nil || ok {
										t.Fatalf("absent=%v/%v", ok, err)
									}
								}
								if clearFilter {
									r.FlushMatches()
								}
								if refresh {
									if changed, err := r.Refresh(); err != nil || !changed {
										t.Fatalf("population refresh=%v/%v", changed, err)
									}
								}
								want := []uint64{1001}
								if clearFilter && initial != 0 {
									want = []uint64{1000, 1001}
								}
								for _, stamp := range want {
									if err := r.Next(); err != nil {
										t.Fatal(err)
									}
									if got, err := r.GetRealtimeUsec(); err != nil || got != stamp {
										t.Fatalf("raw row=%d/%v want%d", got, err, stamp)
									}
								}
								if err := r.Next(); !errors.Is(err, errEndOfEntries) {
									t.Fatalf("raw EOF=%v", err)
								}
							})
						}
					}
				}
			}
		}
	}
}

func TestNativeFilterValidatesSelectedEntryBodyAfterOpen(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			for _, damage := range []string{"partial-body", "unaligned-body", "outside-arena"} {
				t.Run(fmt.Sprintf("%v/%d/%s", compact, mode, damage), func(t *testing.T) {
					r, path := filterTestReader(t, source, mode, true)
					last := r.entryOffsets[len(r.entryOffsets)-1]
					if damage == "partial-body" {
						if err := os.Truncate(path, int64(last+entryObjectHeaderSize)); err != nil {
							t.Fatal(err)
						}
					} else {
						size := binary.LittleEndian.Uint64(source[last+8:]) + 1
						if damage == "outside-arena" {
							size = r.header.headerSize + r.header.arenaSize
						}
						file, err := os.OpenFile(path, os.O_WRONLY, 0)
						if err != nil {
							t.Fatal(err)
						}
						var buf [8]byte
						binary.LittleEndian.PutUint64(buf[:], size)
						_, err = file.WriteAt(buf[:], int64(last+8))
						closeErr := file.Close()
						if err != nil || closeErr != nil {
							t.Fatalf("modify=%v/%v", err, closeErr)
						}
					}
					r.AddMatch([]byte("SPARSE=some"))
					if err := r.SeekTail(); err != nil {
						t.Fatal(err)
					}
					ok, err := r.StepBack()
					if damage != "partial-body" {
						if ok || !errors.Is(err, errCorruptObject) || r.RecoveryStatus().Incomplete() {
							t.Fatalf("malformed selected row=%v/%v/%+v", ok, err, r.RecoveryStatus())
						}
						return
					}
					if err != nil || !ok {
						t.Fatalf("recovered selection=%v/%v", ok, err)
					}
					if got, err := r.GetRealtimeUsec(); err != nil || got != 1500 {
						t.Fatalf("surviving sparse row=%d/%v", got, err)
					}
					if status := r.RecoveryStatus(); !status.Incomplete() || status.RecoveredEntries != 1004 {
						t.Fatalf("recovery=%+v", status)
					}
				})
			}
		}
	}
}
