package journal

import (
	"bytes"
	"path/filepath"
	"testing"
)

func writeExplorerTestJournal(t *testing.T, path string, compression int) {
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
