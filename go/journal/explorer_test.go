package journal

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var explorerBenchmarkSink int

func writeExplorerTestJournal(t testing.TB, path string, compression int) {
	t.Helper()

	opts := testOptions()
	if compression != CompressionNone {
		opts.Compression = compression
		opts.CompressThresholdBytes = 8
	}
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	defer w.Close()

	rows := []struct {
		fields    []Field
		realtime  uint64
		monotonic uint64
	}{
		{
			fields: []Field{
				StringField("SERVICE", "api"),
				StringField("LEVEL", "i"),
				StringField("MESSAGE", "alpha repeated repeated repeated"),
				StringField("USER", "alice"),
			},
			realtime:  1_000,
			monotonic: 1,
		},
		{
			fields: []Field{
				StringField("SERVICE", "api"),
				StringField("LEVEL", "e"),
				StringField("MESSAGE", "beta repeated repeated repeated"),
				{Name: "BIN", Value: []byte{0x00, 0xff}},
				StringField("USER", "bob"),
			},
			realtime:  2_000,
			monotonic: 2,
		},
		{
			fields: []Field{
				StringField("SERVICE", "db"),
				StringField("LEVEL", "i"),
				StringField("MESSAGE", "gamma repeated repeated repeated"),
				StringField("USER", "carol"),
			},
			realtime:  3_000,
			monotonic: 3,
		},
	}

	for _, row := range rows {
		if err := w.Append(row.fields, EntryOptions{
			RealtimeUsec:  row.realtime,
			MonotonicUsec: row.monotonic,
		}); err != nil {
			t.Fatalf("Append() error = %v", err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestExplorerFilterWithoutFacetsSkipsRowPayloads(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionNone)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerQuery(ExplorerQuery{
		Filters: []ExplorerFilter{FieldIn([]byte("SERVICE"), []byte("api"))},
		Display: DisplayNone(),
		Limit:   Limit(10),
	})
	if err != nil {
		t.Fatalf("ExplorerQuery() error = %v", err)
	}

	if len(result.Rows) != 2 {
		t.Fatalf("rows = %d, want 2", len(result.Rows))
	}
	if result.TotalCandidates != 2 {
		t.Fatalf("total candidates = %d, want 2", result.TotalCandidates)
	}
	if result.Counters.PayloadsMaterialized != 0 {
		t.Fatalf("payloads materialized = %d, want 0", result.Counters.PayloadsMaterialized)
	}
	if result.Counters.CandidateDataRefsVisited != 0 {
		t.Fatalf("candidate data refs visited = %d, want 0", result.Counters.CandidateDataRefsVisited)
	}
}

func TestExplorerSelectedFacetMaterializesOnlyFacetValues(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionNone)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerQuery(ExplorerQuery{
		Facets:  [][]byte{[]byte("LEVEL")},
		Display: DisplayNone(),
		Limit:   Limit(0),
	})
	if err != nil {
		t.Fatalf("ExplorerQuery() error = %v", err)
	}

	want := []ExplorerFacetValue{
		{Value: []byte("e"), Count: 1},
		{Value: []byte("i"), Count: 2},
	}
	if len(result.Facets) != 1 || !bytes.Equal(result.Facets[0].Field, []byte("LEVEL")) {
		t.Fatalf("facets = %#v, want LEVEL facet", result.Facets)
	}
	if !explorerFacetValuesEqual(result.Facets[0].Values, want) {
		t.Fatalf("facet values = %#v, want %#v", result.Facets[0].Values, want)
	}
	if result.Counters.FacetValuesMaterialized != 3 {
		t.Fatalf("facet values materialized = %d, want 3", result.Counters.FacetValuesMaterialized)
	}
	if result.Counters.PayloadsMaterialized != 3 {
		t.Fatalf("payloads materialized = %d, want 3", result.Counters.PayloadsMaterialized)
	}
}

func TestExplorerConstrainedFacetFallsBackForRepeatedFieldExtraValue(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	opts := testOptions()
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if err := w.AppendRaw([][]byte{[]byte("TAG=a"), []byte("TAG=b")}, EntryOptions{RealtimeUsec: 1_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("AppendRaw(first) error = %v", err)
	}
	if err := w.AppendRaw([][]byte{[]byte("TAG=a")}, EntryOptions{RealtimeUsec: 2_000, MonotonicUsec: 2}); err != nil {
		t.Fatalf("AppendRaw(second) error = %v", err)
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerQuery(ExplorerQuery{
		Filters: []ExplorerFilter{FieldIn([]byte("TAG"), []byte("a"))},
		Facets:  [][]byte{[]byte("TAG")},
		Display: DisplayNone(),
		Limit:   Limit(10),
	})
	if err != nil {
		t.Fatalf("ExplorerQuery() error = %v", err)
	}

	want := []ExplorerFacetValue{{Value: []byte("a"), Count: 2}, {Value: []byte("b"), Count: 1}}
	if len(result.Facets) != 1 || !explorerFacetValuesEqual(result.Facets[0].Values, want) {
		t.Fatalf("facet values = %#v, want %#v", result.Facets, want)
	}
	if result.Counters.CandidateDataRefsVisited != 3 {
		t.Fatalf("candidate data refs visited = %d, want 3", result.Counters.CandidateDataRefsVisited)
	}
}

func TestExplorerSkipsCompressedIrrelevantFields(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionZSTD)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerQuery(ExplorerQuery{
		Facets:  [][]byte{[]byte("LEVEL")},
		Display: DisplayNone(),
		Limit:   Limit(0),
	})
	if err != nil {
		t.Fatalf("ExplorerQuery() error = %v", err)
	}

	if result.Counters.FacetValuesMaterialized != 3 {
		t.Fatalf("facet values materialized = %d, want 3", result.Counters.FacetValuesMaterialized)
	}
	if result.Counters.PayloadsDecompressed != 0 {
		t.Fatalf("payloads decompressed = %d, want 0", result.Counters.PayloadsDecompressed)
	}
}

func TestExplorerFilteredUniqueUsesTargetFieldChain(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionNone)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerUnique(ExplorerUniqueQuery{
		Field:         []byte("USER"),
		Filters:       []ExplorerFilter{FieldIn([]byte("SERVICE"), []byte("api"))},
		IncludeCounts: true,
	})
	if err != nil {
		t.Fatalf("ExplorerUnique() error = %v", err)
	}

	if len(result.Values) != 2 {
		t.Fatalf("unique values = %d, want 2", len(result.Values))
	}
	if !bytes.Equal(result.Values[0].Value, []byte("alice")) || result.Values[0].Count == nil || *result.Values[0].Count != 1 {
		t.Fatalf("first value = %#v, want alice:1", result.Values[0])
	}
	if !bytes.Equal(result.Values[1].Value, []byte("bob")) || result.Values[1].Count == nil || *result.Values[1].Count != 1 {
		t.Fatalf("second value = %#v, want bob:1", result.Values[1])
	}
	if result.Counters.PayloadsMaterialized != 2 {
		t.Fatalf("payloads materialized = %d, want 2", result.Counters.PayloadsMaterialized)
	}
}

func TestExplorerFilteredUniqueSortsBeforePagination(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionNone)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()

	result, err := r.ExplorerUnique(ExplorerUniqueQuery{
		Field: []byte("USER"),
		Limit: Limit(1),
		Skip:  1,
	})
	if err != nil {
		t.Fatalf("ExplorerUnique() error = %v", err)
	}

	if len(result.Values) != 1 || !bytes.Equal(result.Values[0].Value, []byte("bob")) {
		t.Fatalf("values = %#v, want bob", result.Values)
	}
}

func TestExplorerDataRefsReportOffsetsWithoutPayloadMaterialization(t *testing.T) {
	path := filepath.Join(t.TempDir(), "system.journal")
	writeExplorerTestJournal(t, path, CompressionNone)

	r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer r.Close()
	if err := r.Next(); err != nil {
		t.Fatalf("Next() error = %v", err)
	}
	var refs []EntryDataRef
	if err := r.VisitEntryDataRefs(func(ref EntryDataRef) error {
		refs = append(refs, ref)
		return nil
	}); err != nil {
		t.Fatalf("VisitEntryDataRefs() error = %v", err)
	}
	if len(refs) != 4 {
		t.Fatalf("refs = %d, want 4", len(refs))
	}
	for _, ref := range refs {
		if ref.Offset == 0 || ref.PayloadLen == 0 {
			t.Fatalf("invalid ref %#v", ref)
		}
	}
}

func TestDirectoryExplorerUsesDirectoryReaderTieOrdering(t *testing.T) {
	dir := t.TempDir()
	journals := filepath.Join(dir, "journals")
	if err := os.MkdirAll(journals, 0o755); err != nil {
		t.Fatalf("MkdirAll() error = %v", err)
	}
	firstPath := filepath.Join(journals, "system.journal")
	secondPath := filepath.Join(journals, "user.journal")

	first, err := Create(firstPath, testOptions())
	if err != nil {
		t.Fatalf("Create(first) error = %v", err)
	}
	if err := first.AppendRaw([][]byte{[]byte("SERVICE=target"), []byte("ID=seq1-late")}, EntryOptions{RealtimeUsec: 2_000, MonotonicUsec: 1}); err != nil {
		t.Fatalf("AppendRaw(first) error = %v", err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("Close(first) error = %v", err)
	}

	second, err := Create(secondPath, testOptions())
	if err != nil {
		t.Fatalf("Create(second) error = %v", err)
	}
	if err := second.AppendRaw([][]byte{[]byte("SERVICE=noise"), []byte("ID=ignored")}, EntryOptions{RealtimeUsec: 500, MonotonicUsec: 1}); err != nil {
		t.Fatalf("AppendRaw(noise) error = %v", err)
	}
	if err := second.AppendRaw([][]byte{[]byte("SERVICE=target"), []byte("ID=seq2-early")}, EntryOptions{RealtimeUsec: 1_000, MonotonicUsec: 2}); err != nil {
		t.Fatalf("AppendRaw(second) error = %v", err)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("Close(second) error = %v", err)
	}

	reader, err := OpenDirectoryWithOptions(journals, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		t.Fatalf("OpenDirectoryWithOptions() error = %v", err)
	}
	defer reader.Close()

	result, err := reader.ExplorerQuery(ExplorerQuery{
		Filters: []ExplorerFilter{FieldIn([]byte("SERVICE"), []byte("target"))},
		Display: DisplayFields([]byte("ID")),
		Limit:   Limit(2),
	})
	if err != nil {
		t.Fatalf("ExplorerQuery() error = %v", err)
	}
	var ids [][]byte
	for _, row := range result.Rows {
		for _, field := range row.Fields {
			if bytes.Equal(field.Name, []byte("ID")) {
				ids = append(ids, field.Value)
			}
		}
	}
	want := [][]byte{[]byte("seq1-late"), []byte("seq2-early")}
	if len(ids) != len(want) {
		t.Fatalf("ids = %#v, want %#v", ids, want)
	}
	for i := range want {
		if !bytes.Equal(ids[i], want[i]) {
			t.Fatalf("ids = %#v, want %#v", ids, want)
		}
	}
}

func BenchmarkExplorerCompressedPayloadMaterialization(b *testing.B) {
	path := filepath.Join(b.TempDir(), "system.journal")
	opts := testOptions()
	opts.Compression = CompressionZSTD
	opts.CompressThresholdBytes = 8
	writer, err := Create(path, opts)
	if err != nil {
		b.Fatalf("Create() error = %v", err)
	}
	if err := writer.Append([]Field{
		StringField("MESSAGE", strings.Repeat("compressible-", 512)),
	}, EntryOptions{RealtimeUsec: 1_000, MonotonicUsec: 1}); err != nil {
		b.Fatalf("Append() error = %v", err)
	}
	if err := writer.Close(); err != nil {
		b.Fatalf("Close() error = %v", err)
	}
	reader, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
	if err != nil {
		b.Fatalf("OpenFileWithOptions() error = %v", err)
	}
	defer reader.Close()
	offsets, err := reader.FieldDataOffsets([]byte("MESSAGE"))
	if err != nil {
		b.Fatalf("FieldDataOffsets() error = %v", err)
	}
	if len(offsets) == 0 {
		b.Fatal("no MESSAGE DATA offsets")
	}

	b.Run("generic-readDataPayload", func(b *testing.B) {
		b.ReportAllocs()
		for i := 0; i < b.N; i++ {
			payload, err := reader.readDataPayload(offsets[i%len(offsets)])
			if err != nil {
				b.Fatalf("readDataPayload() error = %v", err)
			}
			explorerBenchmarkSink += len(payload)
		}
	})
	b.Run("explorer-materializePayload", func(b *testing.B) {
		b.ReportAllocs()
		var counters ExplorerQueryCounters
		for i := 0; i < b.N; i++ {
			payload, err := reader.materializePayload(offsets[i%len(offsets)], &counters)
			if err != nil {
				b.Fatalf("materializePayload() error = %v", err)
			}
			explorerBenchmarkSink += len(payload)
		}
	})
}

func explorerFacetValuesEqual(a, b []ExplorerFacetValue) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if !bytes.Equal(a[i].Value, b[i].Value) || a[i].Count != b[i].Count {
			return false
		}
	}
	return true
}
