package journal

import (
	"bytes"
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"
)

// Paired with Rust tests/entry_damage.rs damaged_compressed_data_costs_the_field.
func TestDamagedCompressedDataCostsTheFieldNotTheEntry(t *testing.T) {
	for _, c := range []struct {
		name        string
		compression int
	}{{"zstd", CompressionZSTD}, {"xz", CompressionXZ}, {"lz4", CompressionLZ4}} {
		t.Run(c.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "damage.journal")
			opts := testOptions()
			opts.Compression = c.compression
			opts.CompressThresholdBytes = 16
			w, err := Create(path, opts)
			if err != nil {
				t.Fatal(err)
			}
			appendWriterFields(t, w, "entry", []Field{
				StringField("MESSAGE", "kept"),
				{Name: "BROKEN", Value: bytes.Repeat([]byte("z"), 512)},
			}, EntryOptions{RealtimeUsec: 1_700_000_000_000_001, MonotonicUsec: 1})
			closeWriterForTest(t, w, "entry")
			corruptCompressedData(t, path, c.compression == CompressionLZ4)

			r, err := OpenFile(path)
			if err != nil {
				t.Fatal(err)
			}
			defer r.Close()
			if ok, err := r.Step(); err != nil || !ok {
				t.Fatalf("step = %v, %v", ok, err)
			}
			entry, err := r.GetEntry()
			if err != nil {
				t.Fatal(err)
			}
			if string(entry.Fields["MESSAGE"]) != "kept" || len(entry.Payloads) != 1 {
				t.Fatalf("fields = %v, payloads = %d", entry.Fields, len(entry.Payloads))
			}
			payloads, err := r.CollectEntryPayloads()
			if err != nil || len(payloads) != 1 {
				t.Fatalf("collected = %d, %v", len(payloads), err)
			}
			if err := VerifyFile(path); err == nil {
				t.Fatal("verification accepted a damaged compressed DATA object")
			}
		})
	}
}

// corruptCompressedData breaks the only compressed DATA object: lz4 by its
// stored size, the others by their trailing checksum or footer.
func corruptCompressedData(t *testing.T, path string, lz4 bool) {
	t.Helper()
	data := readFileForTest(t, path)
	offset := binary.LittleEndian.Uint64(data[88:])
	tail := binary.LittleEndian.Uint64(data[136:])
	for offset != 0 && offset <= tail {
		size := binary.LittleEndian.Uint64(data[offset+8:])
		if data[offset] == objectTypeData && data[offset+1]&objectCompressedMask != 0 {
			start, end := offset+objectHeaderToPayload, offset+size
			if lz4 {
				binary.LittleEndian.PutUint64(data[start:], binary.LittleEndian.Uint64(data[start:])+1)
			} else {
				copy(data[end-8:end], make([]byte, 8))
			}
			if err := os.WriteFile(path, data, 0o644); err != nil {
				t.Fatal(err)
			}
			return
		}
		offset += (size + 7) &^ 7
	}
	t.Fatal("no compressed DATA object")
}

// Paired with Rust explorer_skips_damaged_compressed_data.
func TestExplorerSkipsDamagedCompressedData(t *testing.T) {
	large := append([]byte("MESSAGE="), bytes.Repeat([]byte("z"), 512)...)
	path := createExplorerRawJournalWithOptions(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("PRIORITY=3"), large}},
	}, func(opts *Options) {
		opts.Compression = CompressionZSTD
		opts.CompressThresholdBytes = 32
	})
	corruptCompressedData(t, path, false)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("PRIORITY")).WithFacet([]byte("MESSAGE"))
	query.UseSourceRealtime = false
	query.Limit = 10
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore() error = %v", err)
	}
	assertExplorerFacetCount(t, result, "PRIORITY", "3", 1)
	if values := result.Facets["MESSAGE"]; len(values) != 1 || values[string(explorerUnsetValue)] != 1 {
		t.Fatalf("MESSAGE facet = %#v, want only unset=1", values)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows = %d, want 1", len(result.Rows))
	}
	for _, payload := range result.Rows[0].Payloads {
		if bytes.HasPrefix(payload, []byte("MESSAGE=")) {
			t.Fatalf("row kept damaged MESSAGE payload %q", payload[:min(len(payload), 32)])
		}
	}
}
