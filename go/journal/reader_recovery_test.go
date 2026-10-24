package journal

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"testing"
)

type recoveryTestArray struct {
	offset  uint64
	entries []uint64
}

func recoveryTestFixture(t *testing.T, compact bool) ([]byte, []recoveryTestArray) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "system.journal")
	opts := testOptions()
	opts.Compact = compact
	w, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 1005; i++ {
		fields := []Field{StringField("MESSAGE", fmt.Sprintf("row-%04d", i)), StringField("COMMON", "all")}
		if i == 0 || i == 500 || i == 1004 {
			fields = append(fields, StringField("SPARSE", "some"))
		}
		if i >= 1003 {
			fields = append(fields, StringField("TAIL_PAIR", "some"))
		}
		if err := w.Append(fields, EntryOptions{RealtimeUsec: 1000 + uint64(i), MonotonicUsec: 100 + uint64(i)}); err != nil {
			t.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	itemSize := uint64(8)
	if compact {
		itemSize = 4
	}
	var arrays []recoveryTestArray
	remaining := binary.LittleEndian.Uint64(data[152:])
	for offset := binary.LittleEndian.Uint64(data[176:]); remaining > 0; {
		capacity := (binary.LittleEndian.Uint64(data[offset+8:]) - 24) / itemSize
		count := minUint64(capacity, remaining)
		a := recoveryTestArray{offset: offset}
		for i := uint64(0); i < count; i++ {
			a.entries = append(a.entries, entryOffsetArrayItem(data[offset+24+i*itemSize:], itemSize))
		}
		arrays = append(arrays, a)
		remaining -= count
		offset = binary.LittleEndian.Uint64(data[offset+16:])
	}
	return data, arrays
}

func recoveryTestPrefix(data []byte, arrays []recoveryTestArray, cut uint64, compact bool) int {
	itemSize := uint64(8)
	if compact {
		itemSize = 4
	}
	count := 0
	for _, a := range arrays {
		for i, offset := range a.entries {
			if a.offset+24+uint64(i+1)*itemSize > cut || offset+binary.LittleEndian.Uint64(data[offset+8:]) > cut {
				return count
			}
			count++
		}
	}
	return count
}

func recoveryTestTimes(t *testing.T, reader *Reader, backward bool) []uint64 {
	t.Helper()
	var times []uint64
	for {
		var ok bool
		var err error
		if backward {
			ok, err = reader.StepBack()
		} else {
			ok, err = reader.Step()
		}
		if err != nil {
			t.Fatal(err)
		}
		if !ok {
			return times
		}
		value, err := reader.GetRealtimeUsec()
		if err != nil {
			t.Fatal(err)
		}
		times = append(times, value)
	}
}

func TestRecoverySnapshotSecondaryPostingShrink(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, arrays := recoveryTestFixture(t, compact)
		for _, mode := range []ReaderAccessMode{ReaderAccessMmap, ReaderAccessReadAt} {
			t.Run(fmt.Sprintf("compact=%v/mode=%d", compact, mode), func(t *testing.T) {
				r, path := filterTestReader(t, source, mode, false)
				data := filterTestDataOffset(t, r, "COMMON=all")
				array := binary.LittleEndian.Uint64(source[data+48:])
				cut := array + 24 + r.offsetArrayItemSize()
				wantCount := recoveryTestPrefix(source, arrays, cut, compact)
				if wantCount != 2 {
					t.Fatalf("fixture retains %d rows, want 2", wantCount)
				}
				r.AddMatch([]byte("COMMON=all"))
				if err := os.Truncate(path, int64(cut)); err != nil {
					t.Fatal(err)
				}
				if got := recoveryTestTimes(t, r, false); !slices.Equal(got, []uint64{1000, 1001}) {
					t.Fatalf("prefix=%v", got)
				}
				if status := r.RecoveryStatus(); !status.Incomplete() || status.RecoveredEntries != 2 {
					t.Fatalf("status=%+v", status)
				}
				if err := os.WriteFile(path, source, 0o600); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("repair: %v/%v", ok, err)
				}
				if got, err := r.GetRealtimeUsec(); err != nil || got != 1002 {
					t.Fatalf("repair time=%d/%v", got, err)
				}
			})
		}
	}
}

type partialArrayExtentAccessor struct {
	readerAccessor
	header []byte
}

func (a *partialArrayExtentAccessor) tempSlice(offset, size uint64) ([]byte, error) {
	if offset != 4096 || size != offsetArrayObjectHeaderSize {
		return nil, errCorruptObject
	}
	return a.header, nil
}

func TestRecoveryPartialArrayRejectsUnrepresentableAllocation(t *testing.T) {
	maxInt := uint64(int(^uint(0) >> 1))
	const offset = uint64(4096)
	bodySize := maxInt + 1 + regularOffsetArrayItemSize
	header := make([]byte, offsetArrayObjectHeaderSize)
	header[0] = objectTypeEntryArray
	binary.LittleEndian.PutUint64(header[8:], offsetArrayObjectHeaderSize+bodySize)
	r := Reader{
		header:   journalHeader{headerSize: headerSize, arenaSize: offset + offsetArrayObjectHeaderSize + bodySize - headerSize},
		fileSize: offset + offsetArrayObjectHeaderSize + maxInt + 1,
		accessor: &partialArrayExtentAccessor{header: header},
	}
	r.configureLayout()
	defer func() {
		if failure := recover(); failure != nil {
			t.Errorf("partial-array allocation panicked: %v", failure)
		}
	}()
	if _, err := r.recoverPartialArray(offset, ^uint64(0)); !errors.Is(err, errCorruptObject) {
		t.Fatalf("oversized recovery allocation: %v", err)
	}
}

func TestRecoveryPrefixReverseTimeAndExactFilter(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		last := arrays[len(arrays)-1]
		lastEntry := last.entries[len(last.entries)-1]
		itemSize := uint64(8)
		if compact {
			itemSize = 4
		}
		for name, cut := range map[string]uint64{"entry-header": lastEntry, "entry-body": lastEntry + 32, "array-header": last.offset, "array-slots": last.offset + 24 + 3*itemSize} {
			t.Run(fmt.Sprintf("%v/%s", compact, name), func(t *testing.T) {
				path := filepath.Join(t.TempDir(), "system.journal")
				if err := os.WriteFile(path, data[:cut], 0o600); err != nil {
					t.Fatal(err)
				}
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				forward := recoveryTestTimes(t, r, false)
				want := recoveryTestPrefix(data, arrays, cut, compact)
				if len(forward) != want {
					t.Fatalf("forward=%d want=%d", len(forward), want)
				}
				status := r.RecoveryStatus()
				if !status.Incomplete() || status.RecoveredEntries != uint64(want) || r.Header().nEntries != 1005 {
					t.Fatalf("status=%+v", status)
				}
				if err := r.SeekTail(); err != nil {
					t.Fatal(err)
				}
				reverse := recoveryTestTimes(t, r, true)
				slices.Reverse(reverse)
				if !slices.Equal(reverse, forward) {
					t.Fatal("reverse differs")
				}
				if err := r.SeekRealtimeUsec(1000 + uint64(want/2)); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("time step: %v %v", ok, err)
				}
				value, err := r.GetRealtimeUsec()
				if err != nil || value != 1000+uint64(want/2) {
					t.Fatalf("time=%d %v", value, err)
				}
				r.AddMatch([]byte("COMMON=all"))
				if err := r.SeekHead(); err != nil {
					t.Fatal(err)
				}
				filtered := recoveryTestTimes(t, r, false)
				if !slices.Equal(filtered, forward) {
					t.Fatalf("filtered=%d forward=%d", len(filtered), len(forward))
				}
				if err := VerifyFile(path); err == nil {
					t.Fatal("VerifyFile accepted truncated journal")
				}
				if err := VerifyIndex(context.Background(), path); err == nil {
					t.Fatal("VerifyIndex accepted truncated journal")
				}
				if snapshot, err := OpenIndexedSnapshot(context.Background(), path, IndexedSnapshotOptions{}); err == nil {
					snapshot.Close()
					t.Fatal("IndexedSnapshot accepted truncated journal")
				}
				before, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				if w, err := Open(path); err == nil {
					w.Close()
					t.Fatal("append-open accepted truncated journal")
				}
				after, err := os.ReadFile(path)
				if err != nil {
					t.Fatal(err)
				}
				if !bytes.Equal(before, after) {
					t.Fatal("rejected append changed bytes")
				}
			})
		}
	}
}

func TestRecoverySparsePostingBoundary(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		cut := arrays[len(arrays)-1].entries[len(arrays[len(arrays)-1].entries)-1]
		payloadStart := uint64(64)
		if compact {
			payloadStart += 8
		}
		var sparse uint64
		for at := binary.LittleEndian.Uint64(data[88:]); at+16 <= uint64(len(data)); {
			size := binary.LittleEndian.Uint64(data[at+8:])
			if size < 16 || size > uint64(len(data))-at {
				break
			}
			if data[at] == 1 && size >= payloadStart && bytes.Equal(data[at+payloadStart:at+size], []byte("SPARSE=some")) {
				sparse = at
				break
			}
			at += (size + 7) &^ 7
		}
		if sparse == 0 {
			t.Fatal("missing sparse DATA object")
		}
		for _, missingArray := range []bool{false, true} {
			t.Run(fmt.Sprintf("%v/missing-array=%v", compact, missingArray), func(t *testing.T) {
				damaged := bytes.Clone(data[:cut])
				if missingArray {
					clear(damaged[sparse+48 : sparse+56])
				}
				path := filepath.Join(t.TempDir(), "system.journal")
				if err := os.WriteFile(path, damaged, 0o600); err != nil {
					t.Fatal(err)
				}
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				query := DefaultExplorerQuery().WithFilter([]byte("SPARSE"), []byte("some")).WithFacet([]byte("SPARSE"))
				query.FieldMode = ExplorerFieldModeAllValues
				query.UseSourceRealtime = false
				result, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
				if missingArray {
					if err == nil {
						t.Fatal("unknown posting targets were silently omitted")
					}
					return
				}
				if err != nil {
					t.Fatal(err)
				}
				assertExplorerFacetCount(t, result, "SPARSE", "some", 2)
			})
		}
	}
}

func TestIndexedPostingsRejectMissingPointersWithoutPriorTraversal(t *testing.T) {
	for _, compact := range []bool{false, true} {
		source, _ := recoveryTestFixture(t, compact)
		payloadStart := uint64(64)
		if compact {
			payloadStart = 72
		}
		var common uint64
		for at := binary.LittleEndian.Uint64(source[88:]); at+16 <= uint64(len(source)); {
			size := binary.LittleEndian.Uint64(source[at+8:])
			if size < 16 || size > uint64(len(source))-at {
				break
			}
			if source[at] == 1 && size >= payloadStart && bytes.Equal(source[at+payloadStart:at+size], []byte("COMMON=all")) {
				common = at
				break
			}
			at += (size + 7) &^ 7
		}
		if common == 0 {
			t.Fatal("missing common DATA object")
		}
		array := binary.LittleEndian.Uint64(source[common+48:])
		for name, pointer := range map[string]uint64{"inline": common + 40, "head": common + 48, "next": array + 16} {
			t.Run(fmt.Sprintf("%v/%s", compact, name), func(t *testing.T) {
				damaged := bytes.Clone(source)
				clear(damaged[pointer : pointer+8])
				path := filepath.Join(t.TempDir(), "system.journal")
				if err := os.WriteFile(path, damaged, 0o600); err != nil {
					t.Fatal(err)
				}
				for _, mode := range []string{"filter", "facet", "histogram"} {
					r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
					if err != nil {
						t.Fatal(err)
					}
					query := DefaultExplorerQuery()
					query.Limit = 0
					query.FieldMode = ExplorerFieldModeAllValues
					query.UseSourceRealtime = false
					switch mode {
					case "filter":
						query.Limit = 1
						query = query.WithFilter([]byte("COMMON"), []byte("all"))
					case "facet":
						query = query.WithFacet([]byte("COMMON"))
					case "histogram":
						query = query.WithHistogram([]byte("COMMON"))
					}
					_, err = r.ExploreWithStrategy(query, ExplorerStrategyIndex)
					if err == nil {
						t.Errorf("%s silently omitted declared postings", mode)
					}
					if r.RecoveryStatus().Incomplete() {
						t.Error("a broken secondary index changed global recovery status")
					}
					r.Close()
				}
			})
		}
	}
}

func TestRecoveryMissingArrayAfterLastSurvivingPosting(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		path := filepath.Join(t.TempDir(), "system.journal")
		if err := os.WriteFile(path, data, 0o600); err != nil {
			t.Fatal(err)
		}
		pristine, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			t.Fatal(err)
		}
		pair, found, err := pristine.findDataOffsetByPayload([]byte("TAIL_PAIR=some"))
		pristine.Close()
		if err != nil || !found {
			t.Fatalf("tail pair lookup: %v/%v", found, err)
		}
		lastArray := arrays[len(arrays)-1]
		last := lastArray.entries[len(lastArray.entries)-1]
		clear(data[last : last+16])
		clear(data[pair+48 : pair+56])
		if err := os.WriteFile(path, data, 0o600); err != nil {
			t.Fatal(err)
		}
		r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			t.Fatal(err)
		}
		query := DefaultExplorerQuery().WithFilter([]byte("TAIL_PAIR"), []byte("some")).WithFacet([]byte("TAIL_PAIR"))
		query.FieldMode = ExplorerFieldModeAllValues
		query.UseSourceRealtime = false
		result, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
		if err != nil {
			t.Fatal(err)
		}
		if len(result.Rows) != 1 || result.Rows[0].RealtimeUsec != 2003 {
			t.Fatalf("unexpected surviving rows: %v", result.Rows)
		}
		assertExplorerFacetCount(t, result, "TAIL_PAIR", "some", 1)
		if r.RecoveryStatus().RecoveredEntries != 1004 {
			t.Fatal("wrong recovered population")
		}
		r.Close()
	}
}

func TestRecoveryRefreshReturnsRepairedTailOnce(t *testing.T) {
	data, arrays := recoveryTestFixture(t, false)
	last := arrays[len(arrays)-1]
	cut := last.entries[len(last.entries)-1]
	for _, explicit := range []bool{false, true} {
		t.Run(fmt.Sprint(explicit), func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "system.journal")
			if err := os.WriteFile(path, data[:cut], 0o600); err != nil {
				t.Fatal(err)
			}
			r, err := OpenFile(path)
			if err != nil {
				t.Fatal(err)
			}
			defer r.Close()
			if got := len(recoveryTestTimes(t, r, false)); got != 1004 {
				t.Fatalf("prefix=%d", got)
			}
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}
			if explicit {
				if changed, err := r.Refresh(); err != nil || !changed {
					t.Fatalf("refresh=%v %v", changed, err)
				}
			}
			tail := recoveryTestTimes(t, r, false)
			if !slices.Equal(tail, []uint64{2004}) {
				t.Fatalf("repaired tail=%v", tail)
			}
			if r.RecoveryStatus().Incomplete() {
				t.Fatal("complete repair still degraded")
			}
		})
	}
}

func TestRecoveryRejectsInteriorHolesAndArenaPointers(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		last := arrays[len(arrays)-1]
		cut := last.entries[len(last.entries)-1]
		item := 8
		if compact {
			item = 4
		}
		for _, corruption := range []string{"zero-entry", "zero-slot", "arena-pointer", "nonzero-header"} {
			t.Run(fmt.Sprintf("%v/%s", compact, corruption), func(t *testing.T) {
				damaged := bytes.Clone(data[:cut])
				first := arrays[0].entries[0]
				switch corruption {
				case "zero-entry", "nonzero-header":
					clear(damaged[first : first+16])
					if corruption == "nonzero-header" {
						damaged[first+1] = 1
					}
				case "zero-slot":
					clear(damaged[arrays[0].offset+24 : arrays[0].offset+24+uint64(item)])
				default:
					bad := binary.LittleEndian.Uint64(data[88:]) + binary.LittleEndian.Uint64(data[96:]) + 8
					slot := arrays[0].offset + 24
					if compact {
						binary.LittleEndian.PutUint32(damaged[slot:], uint32(bad))
					} else {
						binary.LittleEndian.PutUint64(damaged[slot:], bad)
					}
				}
				path := filepath.Join(t.TempDir(), "system.journal")
				if err := os.WriteFile(path, damaged, 0o600); err != nil {
					t.Fatal(err)
				}
				if reader, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true)); err == nil {
					reader.Close()
					t.Fatal("interior corruption recovered")
				}
			})
		}
	}
}

func TestRecoverySameSizePublicationKeepsIndexedQueriesAndRefresh(t *testing.T) {
	for _, compact := range []bool{false, true} {
		data, arrays := recoveryTestFixture(t, compact)
		last := arrays[len(arrays)-1]
		item := uint64(8)
		if compact {
			item = 4
		}
		for _, slot := range []bool{false, true} {
			t.Run(fmt.Sprintf("%v/%v", compact, slot), func(t *testing.T) {
				damaged := bytes.Clone(data)
				if slot {
					at := last.offset + 24 + uint64(len(last.entries)-1)*item
					clear(damaged[at : at+item])
				} else {
					at := last.entries[len(last.entries)-1]
					clear(damaged[at : at+16])
				}
				path := filepath.Join(t.TempDir(), "system.journal")
				if err := os.WriteFile(path, damaged, 0o600); err != nil {
					t.Fatal(err)
				}
				r, err := OpenFile(path)
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				query := DefaultExplorerQuery().WithFilter([]byte("COMMON"), []byte("all")).WithFacet([]byte("COMMON"))
				query.FieldMode = ExplorerFieldModeAllValues
				query.UseSourceRealtime = false
				result, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
				if err != nil {
					t.Fatal(err)
				}
				assertExplorerFacetCount(t, result, "COMMON", "all", 1004)
				r.AddMatch([]byte("MESSAGE=row-1004"))
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("missing exact row=%v %v", ok, err)
				}
				if changed, err := r.Refresh(); err != nil || changed {
					t.Fatalf("unchanged degraded refresh=%v %v", changed, err)
				}
				if err := os.WriteFile(path, data, 0o600); err != nil {
					t.Fatal(err)
				}
				if ok, err := r.Step(); err != nil || !ok {
					t.Fatalf("repaired exact row=%v %v", ok, err)
				}
				if value, err := r.GetRealtimeUsec(); err != nil || value != 2004 {
					t.Fatalf("repaired time=%d %v", value, err)
				}
				if ok, err := r.Step(); err != nil || ok {
					t.Fatalf("duplicate repaired row=%v %v", ok, err)
				}
				if r.RecoveryStatus().Incomplete() {
					t.Fatal("repair still degraded")
				}
			})
		}
	}
}

func TestRecoveryRefreshShrinkKeepsPhysicalBounds(t *testing.T) {
	data, _ := recoveryTestFixture(t, false)
	for _, mode := range readerTestAccessModes() {
		t.Run(accessModeName(mode), func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "system.journal")
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}
			r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(mode))
			if err != nil {
				t.Fatal(err)
			}
			defer r.Close()
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatal(ok, err)
			}
			if _, err := r.GetRealtimeUsec(); err != nil {
				t.Fatal(err)
			}
			if err := os.Truncate(path, 64); err != nil {
				t.Fatal(err)
			}
			if _, err := r.Refresh(); err == nil {
				t.Fatal("header loss accepted")
			}
			if r.AccessStats().FileSize != 64 {
				t.Fatal("stale physical extent")
			}
			if _, err := r.GetRealtimeUsec(); err == nil {
				t.Fatal("stale ENTRY header")
			}
		})
	}
}

func TestRecoveryDirectoryRepairPreservesPendingMergeRows(t *testing.T) {
	for _, compact := range []bool{false, true} {
		dir := t.TempDir()
		aPath, bPath := filepath.Join(dir, "a.journal"), filepath.Join(dir, "b.journal")
		for path, seqs := range map[string][]uint64{aPath: {1, 3}, bPath: {2}} {
			opts := testOptions()
			opts.Compact = compact
			opts.HeadSeqnum = seqs[0]
			w, err := Create(path, opts)
			if err != nil {
				t.Fatal(err)
			}
			for _, seq := range seqs {
				if err := w.Append([]Field{StringField("MESSAGE", fmt.Sprint(seq))}, EntryOptions{Seqnum: seq, RealtimeUsec: seq * 1000, MonotonicUsec: seq}); err != nil {
					t.Fatal(err)
				}
			}
			if err := w.Close(); err != nil {
				t.Fatal(err)
			}
		}
		a, err := OpenFile(aPath)
		if err != nil {
			t.Fatal(err)
		}
		cut := a.entryOffsets[1]
		a.Close()
		bytes, err := os.ReadFile(aPath)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(aPath, bytes[:cut], 0o600); err != nil {
			t.Fatal(err)
		}
		a, err = OpenFile(aPath)
		if err != nil {
			t.Fatal(err)
		}
		b, err := OpenFile(bPath)
		if err != nil {
			t.Fatal(err)
		}
		dr, err := newDirectoryReader([]*Reader{b, a}, false)
		if err != nil {
			t.Fatal(err)
		}
		defer dr.Close()
		if dr.nonOverlapping {
			t.Fatal("recovered prefix proved false disjointness")
		}
		if ok, err := dr.Step(); err != nil || !ok {
			t.Fatal(ok, err)
		}
		if time, err := dr.GetRealtimeUsec(); err != nil || time != 1000 {
			t.Fatal(time, err)
		}
		if err := os.WriteFile(aPath, bytes, 0o600); err != nil {
			t.Fatal(err)
		}
		var times []uint64
		for {
			ok, err := dr.Step()
			if err != nil {
				t.Fatal(err)
			}
			if !ok {
				break
			}
			value, err := dr.GetRealtimeUsec()
			if err != nil {
				t.Fatal(err)
			}
			times = append(times, value)
		}
		if !slices.Equal(times, []uint64{2000, 3000}) {
			t.Fatalf("repair order=%v", times)
		}
		if len(dr.RecoveryStatuses()) != 0 {
			t.Fatal("directory status stale")
		}
		boots, err := dr.ListBoots()
		if err != nil || len(boots) != 1 || boots[0].LastEntry != 3000 {
			t.Fatalf("boots=%v %v", boots, err)
		}
		if err := dr.SeekTail(); err != nil {
			t.Fatal(err)
		}
		times = nil
		for {
			ok, err := dr.StepBack()
			if err != nil {
				t.Fatal(err)
			}
			if !ok {
				break
			}
			value, err := dr.GetRealtimeUsec()
			if err != nil {
				t.Fatal(err)
			}
			times = append(times, value)
		}
		if !slices.Equal(times, []uint64{3000, 2000, 1000}) {
			t.Fatalf("reverse repair order=%v", times)
		}
	}
}
