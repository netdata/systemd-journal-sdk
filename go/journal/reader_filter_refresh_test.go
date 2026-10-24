package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

func TestNativeFilterPendingRecoveryBuildKeepsAllGroups(t *testing.T) {
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
				for i, fields := range [][]Field{{StringField("COMMON", "all")}, {StringField("COMMON", "all"), StringField("TARGET", "only")}} {
					if err := w.Append(fields, EntryOptions{RealtimeUsec: 1000 + uint64(i), MonotonicUsec: 100 + uint64(i)}); err != nil {
						t.Fatal(err)
					}
				}
				if err := w.Close(); err != nil {
					t.Fatal(err)
				}
				source, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				target := filterTestDataOffset(t, r, "TARGET=only")
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				defer file.Close()
				if err := file.Truncate(int64(target + 8)); err != nil {
					t.Fatal(err)
				}
				if _, err := r.Refresh(); err != nil {
					t.Fatal(err)
				}
				if !r.RecoveryStatus().Incomplete() {
					t.Fatal("expected one-row prefix")
				}
				r.AddMatch([]byte("COMMON=all"))
				if err := r.AddConjunction(); err != nil {
					t.Fatal(err)
				}
				partial := bytes.Clone(source)
				binary.LittleEndian.PutUint64(partial[target+40:], 0)
				if _, err := file.WriteAt(partial, 0); err != nil {
					t.Fatal(err)
				}
				r.AddMatch([]byte("TARGET=only"))
				if ok, err := r.Step(); ok || err == nil {
					t.Fatalf("broken DATA=%v/%v", ok, err)
				}
				if r.RecoveryStatus().Incomplete() {
					t.Fatal("ENTRY repair was not observed")
				}
				if _, err := file.WriteAt(source[target+40:target+48], int64(target+40)); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("repaired pending AND=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
					t.Fatalf("row=%d/%v", got, err)
				}
			})
		}
	}
}

func TestNativeFilterKnownEmptyDataKeepsRecoveryIdentity(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("%v/%d", compact, mode), func(t *testing.T) {
				r, path := filterTestReader(t, source, mode, true)
				common := filterTestDataOffset(t, r, "COMMON=all")
				last := r.entryOffsets[len(r.entryOffsets)-1]
				r.AddMatch([]byte("COMMON=all"))
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("initial=%v/%v", ok, err)
				}
				partial := bytes.Clone(source)
				clear(partial[common+40 : common+64])
				clear(partial[last : last+16])
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				defer file.Close()
				if _, err := file.WriteAt(partial, 0); err != nil {
					t.Fatal(err)
				}
				if err := file.Truncate(int64(last + 8)); err != nil {
					t.Fatal(err)
				}
				if _, err := r.Refresh(); err != nil {
					t.Fatal(err)
				}
				if !r.RecoveryStatus().Incomplete() {
					t.Fatal("expected missing tail")
				}
				if err := r.SeekHead(); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("empty retained DATA=%v/%v", ok, err)
				}
				if _, err := file.WriteAt(source, 0); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("repaired retained DATA=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1000 {
					t.Fatalf("row=%d/%v", got, err)
				}
			})
		}
	}
}

func nativeFilterLiveReader(t *testing.T, mode ReaderAccessMode, initial int) (*Writer, *Reader, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "system.journal")
	w, err := Create(path, testOptions())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = w.Close() })
	for i := 0; i < initial; i++ {
		if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 1000 + uint64(i), MonotonicUsec: 100 + uint64(i)}); err != nil {
			t.Fatal(err)
		}
	}
	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = r.Close() })
	return w, r, path
}

func TestNativeFilterIORetryPreservesCapturedPopulation(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		t.Run(fmt.Sprint(mode), func(t *testing.T) {
			w, r, _ := nativeFilterLiveReader(t, mode, 2)
			r.AddMatch([]byte("COMMON=all"))
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("first=%v/%v", ok, err)
			}
			if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 1002, MonotonicUsec: 102}); err != nil {
				t.Fatal(err)
			}
			trace := &filterAccessTrace{readerAccessor: r.accessor, faultAt: r.entryOffsets[1], fail: true}
			r.accessor = trace
			if ok, err := r.Step(); ok || !errors.Is(err, syscall.EIO) {
				t.Fatalf("injected failure=%v/%v", ok, err)
			}
			trace.fail = false
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("retry=%v/%v", ok, err)
			}
			if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
				t.Fatalf("retry row=%d/%v", got, err)
			}
			if ok, err := r.Step(); err != nil || ok {
				t.Fatalf("installed population widened=%v/%v", ok, err)
			}
		})
	}
}

func TestNativeFilterRealtimeRecoveryRebuildsBeforeFirstResult(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		for _, backward := range []bool{false, true} {
			t.Run(fmt.Sprintf("%d/backward=%v", mode, backward), func(t *testing.T) {
				w, r, path := nativeFilterLiveReader(t, mode, 1)
				if err := w.Append([]Field{StringField("TARGET", "new")}, EntryOptions{RealtimeUsec: 1001, MonotonicUsec: 101}); err != nil {
					t.Fatal(err)
				}
				probe, err := OpenFile(path)
				if err != nil {
					t.Fatal(err)
				}
				last := probe.entryOffsets[1]
				if err := probe.Close(); err != nil {
					t.Fatal(err)
				}
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				_, err = file.WriteAt(make([]byte, 16), int64(last))
				closeErr := file.Close()
				if err != nil || closeErr != nil {
					t.Fatalf("remove tail=%v/%v", err, closeErr)
				}
				r.AddMatch([]byte("TARGET=new"))
				if err := r.SeekRealtimeUsec(1000); err != nil {
					t.Fatal(err)
				}
				var ok bool
				if backward {
					ok, err = r.StepBack()
				} else {
					ok, err = r.Step()
				}
				if err != nil || ok {
					t.Fatalf("first recovered result=%v/%v", ok, err)
				}
				if status := r.RecoveryStatus(); !status.Incomplete() || status.RecoveredEntries != 1 {
					t.Fatalf("recovery=%+v", status)
				}
			})
		}
	}
}

func TestNativeFilterRecoveryViewStaysConsistentAcrossLeaves(t *testing.T) {
	w, r, path := nativeFilterLiveReader(t, ReaderAccessReadAt, 1)
	if err := w.Append([]Field{StringField("COMMON", "all"), StringField("TARGET", "only")}, EntryOptions{RealtimeUsec: 1001, MonotonicUsec: 101}); err != nil {
		t.Fatal(err)
	}
	r.AddMatch([]byte("COMMON=all"))
	r.AddMatch([]byte("TARGET=only"))
	if err := r.SeekTail(); err != nil {
		t.Fatal(err)
	}
	if ok, err := r.Step(); err != nil || ok {
		t.Fatalf("build at tail=%v/%v", ok, err)
	}
	source, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	target := filterTestDataOffset(t, r, "TARGET=only")
	if err := r.SeekHead(); err != nil {
		t.Fatal(err)
	}
	if err := os.Truncate(path, int64(target)); err != nil {
		t.Fatal(err)
	}
	if _, err := r.Refresh(); err != nil {
		t.Fatal(err)
	}
	if ok, err := r.Step(); err == nil || ok {
		t.Fatalf("missing required DATA=%v/%v", ok, err)
	}
	file, err := os.OpenFile(path, os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	_, err = file.WriteAt(source, 0)
	closeErr := file.Close()
	if err != nil || closeErr != nil {
		t.Fatalf("repair=%v/%v", err, closeErr)
	}
	if ok, err := r.Step(); err != nil || !ok {
		t.Fatalf("consistent repair=%v/%v", ok, err)
	}
	if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
		t.Fatalf("repaired row=%d/%v", got, err)
	}
	if r.RecoveryStatus().Incomplete() {
		t.Fatal("complete repair remained degraded")
	}
}

func TestNativeFilterSnapshotRestoresOriginalExtent(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("compact=%v/%d", compact, mode), func(t *testing.T) {
				r, path := filterTestReader(t, source, mode, false)
				last := r.entryOffsets[len(r.entryOffsets)-1]
				if err := os.Truncate(path, int64(last+8)); err != nil {
					t.Fatal(err)
				}
				r.AddMatch([]byte("SPARSE=some"))
				if err := r.SeekTail(); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.StepBack(); err != nil || !ok || !r.RecoveryStatus().Incomplete() {
					t.Fatalf("observed shrink=%v/%v/%+v", ok, err, r.RecoveryStatus())
				}
				file, err := os.OpenFile(path, os.O_WRONLY, 0)
				if err != nil {
					t.Fatal(err)
				}
				_, err = file.WriteAt(append(bytes.Clone(source), make([]byte, 4096)...), 0)
				closeErr := file.Close()
				if err != nil || closeErr != nil {
					t.Fatalf("restore and grow=%v/%v", err, closeErr)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("restored snapshot=%v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 2004 {
					t.Fatalf("restored row=%d/%v", got, err)
				}
				if status := r.RecoveryStatus(); status.Incomplete() {
					t.Fatalf("restored status=%+v", status)
				}
				if got := r.accessor.size(); got != uint64(len(source)) {
					t.Fatalf("snapshot extent=%d want%d", got, len(source))
				}
			})
		}
	}
}

func TestNativeFilterSnapshotNewBuilderSeesPreallocatedPublication(t *testing.T) {
	for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
		path := filepath.Join(t.TempDir(), "system.journal")
		w, err := Create(path, testOptions())
		if err != nil {
			t.Fatal(err)
		}
		if err := w.Append([]Field{StringField("COMMON", "all")}, EntryOptions{RealtimeUsec: 1000, MonotonicUsec: 100}); err != nil {
			t.Fatal(err)
		}
		r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode).WithSnapshot(true))
		if err != nil {
			t.Fatal(err)
		}
		if err := w.Append([]Field{StringField("TARGET", "new")}, EntryOptions{RealtimeUsec: 1001, MonotonicUsec: 101}); err != nil {
			t.Fatal(err)
		}
		r.AddMatch([]byte("TARGET=new"))
		if err := r.SeekRealtimeUsec(1000); err != nil {
			t.Fatal(err)
		}
		if ok, err := r.Step(); err != nil || !ok {
			t.Fatalf("new snapshot filter=%v/%v", ok, err)
		}
		if got, err := r.GetRealtimeUsec(); err != nil || got != 1001 {
			t.Fatalf("new snapshot row=%d/%v", got, err)
		}
		if err := r.Close(); err != nil {
			t.Fatal(err)
		}
		if err := w.Close(); err != nil {
			t.Fatal(err)
		}
	}
}
