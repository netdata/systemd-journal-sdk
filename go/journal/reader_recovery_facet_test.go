package journal

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func recoveryUnusedArrayReader(tb testing.TB, compact bool) *Reader {
	tb.Helper()
	path := filepath.Join(tb.TempDir(), "system.journal")
	opts := testOptions()
	opts.Compact = compact
	w, err := Create(path, opts)
	if err != nil {
		tb.Fatal(err)
	}
	tb.Cleanup(func() { _ = w.Close() })
	for i, fields := range [][]Field{
		{StringField("BASE", "one")},
		{{Name: "ORPHAN", Value: bytes.Repeat([]byte("x"), 512<<10)}},
	} {
		if err := w.Append(fields, testEntryOptions(uint64(i+1))); err != nil {
			tb.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		tb.Fatal(err)
	}
	r, err := OpenFile(path)
	if err != nil {
		tb.Fatal(err)
	}
	header := r.header
	first := r.entryOffsets[0]
	entry, err := r.readEntryHeaderAt(first)
	if err != nil {
		_ = r.Close()
		tb.Fatal(err)
	}
	if err := r.Close(); err != nil {
		tb.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		tb.Fatal(err)
	}
	itemSize := uint64(8)
	if compact {
		itemSize = 4
	}
	array := (uint64(len(data)) + 7) &^ 7
	if array > uint64(^uint32(0)) {
		tb.Fatal("fixture exceeds cached-array offset width")
	}
	// Publish one entry, retaining prepared DATA beyond that population.
	header.nEntries = 1
	header.entryArrayOffset = array
	header.tailObjectOffset = array
	header.nObjects++
	header.nEntryArrays++
	header.tailEntryArrayOffset = uint32(array)
	header.tailEntryArrayNEntries = 1
	header.tailEntryOffset = first
	header.tailEntrySeqnum = entry.seqnum
	header.tailEntryRealtime = entry.realtime
	header.tailEntryMonotonic = entry.monotonic
	header.tailEntryBootID = entry.bootID
	header.arenaSize = array + 24 + 2*itemSize - header.headerSize
	// The committed slot survives; only the unused second slot is unavailable.
	data = append(data, make([]byte, int(array+24+itemSize)-len(data))...)
	putHeader(data, header)
	data[array] = objectTypeEntryArray
	binary.LittleEndian.PutUint64(data[array+8:], 24+2*itemSize)
	if compact {
		binary.LittleEndian.PutUint32(data[array+24:], uint32(first))
	} else {
		binary.LittleEndian.PutUint64(data[array+24:], first)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		tb.Fatal(err)
	}
	r, err = OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		tb.Fatal(err)
	}
	tb.Cleanup(func() { _ = r.Close() })
	status := r.RecoveryStatus()
	if status.Reason != RecoveryMissingEntryArray || status.DeclaredEntries != 1 || status.RecoveredEntries != 1 || len(r.entryOffsets) != 1 {
		tb.Fatalf("unexpected recovered population: %+v", status)
	}
	return r
}

func recoveryUnusedArrayQuery() ExplorerQuery {
	query := DefaultExplorerQuery().WithFacet([]byte("ORPHAN"))
	query.Limit = 0
	query.FieldMode = ExplorerFieldModeAllValues
	query.UseSourceRealtime = false
	return query
}

func TestRecoveryEqualCountFacetWithoutPostings(t *testing.T) {
	for _, compact := range []bool{false, true} {
		t.Run(fmt.Sprint(compact), func(t *testing.T) {
			r := recoveryUnusedArrayReader(t, compact)
			result, err := r.ExploreWithStrategy(recoveryUnusedArrayQuery(), ExplorerStrategyIndex)
			if err != nil {
				t.Fatal(err)
			}
			want := map[string]map[string]uint64{"ORPHAN": {"-": 1}}
			if !reflect.DeepEqual(result.Facets, want) || len(result.Rows) != 0 || result.Stats.DataRefsSeen != 0 || result.Stats.FacetUpdates != 1 {
				t.Fatalf("unexpected recovered facet: %+v", result)
			}
		})
	}
}

func BenchmarkRecoveryUnmatchedFacet(b *testing.B) {
	for _, compact := range []bool{false, true} {
		b.Run(fmt.Sprint(compact), func(b *testing.B) {
			r := recoveryUnusedArrayReader(b, compact)
			query := recoveryUnusedArrayQuery()
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				result, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
				if err != nil {
					b.Fatal(err)
				}
				values := result.Facets["ORPHAN"]
				if len(values) != 1 || values["-"] != 1 || result.Stats.DataRefsSeen != 0 || result.Stats.FacetUpdates != 1 {
					b.Fatal("incorrect recovered facet")
				}
			}
			b.StopTimer()
		})
	}
}
