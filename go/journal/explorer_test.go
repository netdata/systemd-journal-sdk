package journal

import (
	"errors"
	"fmt"
	"path/filepath"
	"testing"
	"time"
)

func TestExplorerTraversalFacetsHistogramFiltersAndRows(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("MESSAGE=alpha"), []byte("SERVICE=api"), []byte("PRIORITY=6")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("MESSAGE=beta"), []byte("SERVICE=api"), []byte("PRIORITY=5")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("MESSAGE=gamma"), []byte("SERVICE=worker"), []byte("PRIORITY=6")}},
		{realtime: 4_000, payloads: [][]byte{[]byte("MESSAGE=error alpha"), []byte("SERVICE=api"), []byte("PRIORITY=6")}},
		{realtime: 5_000, payloads: [][]byte{[]byte("MESSAGE=debug"), []byte("SERVICE=worker"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("api")).
		WithFacet([]byte("PRIORITY")).
		WithFacet([]byte("SERVICE")).
		WithHistogram([]byte("PRIORITY")).
		WithFTSPattern([]byte("alpha"))
	query.UseSourceRealtime = false
	query.Limit = 10

	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore() error = %v", err)
	}
	if got := len(result.Rows); got != 2 {
		t.Fatalf("rows returned = %d, want 2", got)
	}
	assertExplorerFacetCount(t, result, "PRIORITY", "6", 2)
	assertExplorerFacetCount(t, result, "SERVICE", "api", 2)
	if got := explorerHistogramTotal(result, "6"); got != 2 {
		t.Fatalf("histogram PRIORITY=6 total = %d, want 2", got)
	}
	if result.Stats.ReturnedRowExpansions != 2 {
		t.Fatalf("returned row expansions = %d, want 2", result.Stats.ReturnedRowExpansions)
	}
}

func TestExplorerFirstValueIgnoresDuplicateFieldValues(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("MESSAGE=one"), []byte("TAG=a"), []byte("TAG=b")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("MESSAGE=two"), []byte("TAG=b")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	firstValue := DefaultExplorerQuery().WithFacet([]byte("TAG"))
	firstValue.Limit = 0
	result, err := reader.Explore(firstValue)
	if err != nil {
		t.Fatalf("Explore(first-value) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "TAG", "a", 1)
	assertExplorerFacetCount(t, result, "TAG", "b", 1)

	allValues := firstValue
	allValues.FieldMode = ExplorerFieldModeAllValues
	result, err = reader.Explore(allValues)
	if err != nil {
		t.Fatalf("Explore(all-values) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "TAG", "a", 1)
	assertExplorerFacetCount(t, result, "TAG", "b", 2)
}

func TestExplorerIndexCompareMatchesTraversal(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("MESSAGE=one"), []byte("SERVICE=api"), []byte("PRIORITY=6")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("MESSAGE=two"), []byte("SERVICE=api"), []byte("PRIORITY=5")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("MESSAGE=three"), []byte("SERVICE=worker"), []byte("PRIORITY=6")}},
		{realtime: 4_000, payloads: [][]byte{[]byte("MESSAGE=four"), []byte("SERVICE=api"), []byte("PRIORITY=6")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("api")).
		WithFacet([]byte("PRIORITY")).
		WithHistogram([]byte("PRIORITY"))
	query.UseSourceRealtime = false
	query.FieldMode = ExplorerFieldModeAllValues
	query.Limit = 2

	result, err := reader.ExploreWithStrategy(query, ExplorerStrategyCompare)
	if err != nil {
		t.Fatalf("ExploreWithStrategy(compare) error = %v", err)
	}
	if result.Comparison == nil {
		t.Fatal("comparison diagnostics are nil")
	}
	assertExplorerFacetCount(t, result, "PRIORITY", "6", 2)
	assertExplorerFacetCount(t, result, "PRIORITY", "5", 1)
	if got := explorerHistogramTotal(result, "6"); got != 2 {
		t.Fatalf("histogram PRIORITY=6 total = %d, want 2", got)
	}
}

func TestExplorerRejectsDebugColumnTraversal(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("MESSAGE=one")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery()
	query.DebugCollectColumnFieldsByRowTraversal = true
	_, err := reader.Explore(query)
	if !errors.Is(err, errExplorerDebugDisabled) {
		t.Fatalf("Explore(debug column traversal) error = %v, want %v", err, errExplorerDebugDisabled)
	}
}

func TestExplorerControlReportsProgressAndCancellation(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)

	reader := mustOpenReaderFile(t, path)
	defer reader.Close()
	var reports []uint64
	control := NewExplorerControl()
	control.SetProgressInterval(0)
	control.SetProgressCallback(func(progress ExplorerProgress) {
		reports = append(reports, progress.Stats.RowsExamined)
	})
	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.Limit = 0
	result, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyTraversal, control)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(progress) error = %v", err)
	}
	if control.StopReason() != ExplorerStopNone {
		t.Fatalf("stop reason = %v, want none", control.StopReason())
	}
	if result.Stats.RowsExamined != 9_000 {
		t.Fatalf("rows examined = %d, want 9000", result.Stats.RowsExamined)
	}
	if len(reports) == 0 {
		t.Fatal("progress callback was not called")
	}

	reader = mustOpenReaderFile(t, path)
	defer reader.Close()
	cancelControl := NewExplorerControl()
	cancelControl.SetCancellationCallback(func() bool { return true })
	cancelled, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyTraversal, cancelControl)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(cancel) error = %v", err)
	}
	if cancelControl.StopReason() != ExplorerStopCancelled {
		t.Fatalf("stop reason = %v, want cancelled", cancelControl.StopReason())
	}
	if cancelled.Stats.RowsExamined >= 9_000 {
		t.Fatalf("cancelled rows examined = %d, want less than 9000", cancelled.Stats.RowsExamined)
	}
}

func TestExplorerControlReportsTimeout(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	deadline := time.Now().Add(-time.Second)
	control := NewExplorerControl()
	control.SetDeadline(&deadline)
	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.Limit = 0
	_, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyTraversal, control)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(timeout) error = %v", err)
	}
	if control.StopReason() != ExplorerStopTimedOut {
		t.Fatalf("stop reason = %v, want %v", control.StopReason(), ExplorerStopTimedOut)
	}
}

func TestExplorerSkipsIrrelevantCompressedDataForFacets(t *testing.T) {
	largeMessage := []byte("MESSAGE=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz")
	path := createExplorerRawJournalWithOptions(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("PRIORITY=3"), largeMessage}},
	}, func(opts *Options) {
		opts.Compression = CompressionZSTD
		opts.CompressThresholdBytes = 32
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("PRIORITY"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(compressed skip) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "PRIORITY", "3", 1)
	if result.Stats.PayloadsDecompressed != 0 {
		t.Fatalf("payloads decompressed = %d, want 0", result.Stats.PayloadsDecompressed)
	}
	if result.Stats.EarlyStops != 1 {
		t.Fatalf("early stops = %d, want 1", result.Stats.EarlyStops)
	}
}

func TestExplorerSameFieldFilterExclusionCountsFilteredOutFacetValues(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=3")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("a")).
		WithFilter([]byte("PRIORITY"), []byte("3")).
		WithFacet([]byte("SERVICE")).
		WithFacet([]byte("PRIORITY"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(same field exclusion) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "b", 1)
	assertExplorerFacetCount(t, result, "PRIORITY", "3", 1)
	assertExplorerFacetCount(t, result, "PRIORITY", "4", 1)
}

func TestExplorerFTSDisablesFirstValueEarlyStop(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("TAG=one"), []byte("MESSAGE=needle")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("TAG")).WithFTSPattern([]byte("needle"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(fts no early stop) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "TAG", "one", 1)
	if result.Stats.EarlyStops != 0 {
		t.Fatalf("early stops = %d, want 0", result.Stats.EarlyStops)
	}
	if result.Stats.DataRefsSeen != 2 {
		t.Fatalf("data refs seen = %d, want 2", result.Stats.DataRefsSeen)
	}
}

func TestOffsetClassCacheRejectsZeroOffsetAndRoundTripsClasses(t *testing.T) {
	cache := newOffsetClassCache()
	cache.insert(0, offsetClass{kind: offsetClassValue, valueIndex: 99})
	if _, ok := cache.lookup(0); ok {
		t.Fatal("lookup(0) succeeded, want empty sentinel miss")
	}

	cases := []struct {
		offset uint64
		class  offsetClass
	}{
		{offset: 8, class: offsetClass{kind: offsetClassIrrelevant}},
		{offset: 16, class: offsetClass{kind: offsetClassFtsMatch}},
		{offset: 24, class: offsetClass{kind: offsetClassFtsNegative}},
		{offset: 32, class: offsetClass{kind: offsetClassValue, valueIndex: 7}},
	}
	for _, tc := range cases {
		cache.insert(tc.offset, tc.class)
	}
	for i := 0; i < 512; i++ {
		cache.insert(uint64(i+10)*64, offsetClass{kind: offsetClassValue, valueIndex: i % 9})
	}
	for _, tc := range cases {
		got, ok := cache.lookup(tc.offset)
		if !ok {
			t.Fatalf("lookup(%d) missed after cache growth", tc.offset)
		}
		if got.kind != tc.class.kind || got.valueIndex != tc.class.valueIndex {
			t.Fatalf("lookup(%d) = %+v, want %+v", tc.offset, got, tc.class)
		}
	}
}

func TestExplorerUnsetFacetAccountingFinalizesWithoutPerRowLoop(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("A=one")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("B=two")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("MESSAGE=none")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFacet([]byte("A")).
		WithFacet([]byte("B")).
		WithFacet([]byte("C"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(unset facets) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "A", "one", 1)
	assertExplorerFacetCount(t, result, "A", "-", 2)
	assertExplorerFacetCount(t, result, "B", "two", 1)
	assertExplorerFacetCount(t, result, "B", "-", 2)
	assertExplorerFacetCount(t, result, "C", "-", 3)
	if result.Stats.FacetRowsMatched != 3 {
		t.Fatalf("facet rows matched = %d, want 3", result.Stats.FacetRowsMatched)
	}
	if result.Stats.FacetUpdates != 9 {
		t.Fatalf("facet updates = %d, want 9", result.Stats.FacetUpdates)
	}
}

func TestExplorerBackwardTimeBoundStopsAfterSlackWindow(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 100_000_000, payloads: [][]byte{[]byte("SERVICE=a")}},
		{realtime: 200_000_000, payloads: [][]byte{[]byte("SERVICE=b")}},
		{realtime: 300_000_000, payloads: [][]byte{[]byte("SERVICE=c")}},
		{realtime: 400_000_000, payloads: [][]byte{[]byte("SERVICE=d")}},
		{realtime: 500_000_000, payloads: [][]byte{[]byte("SERVICE=e")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after := uint64(350_000_000)
	query := DefaultExplorerQuery()
	query.AfterRealtimeUsec = &after
	query.Direction = DirectionBackward
	query.Limit = 10
	query.RealtimeSlackUsec = 10_000_000
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(backward bound) error = %v", err)
	}
	if got := len(result.Rows); got != 2 {
		t.Fatalf("rows returned = %d, want 2", got)
	}
	if result.Rows[0].RealtimeUsec != 500_000_000 || result.Rows[1].RealtimeUsec != 400_000_000 {
		t.Fatalf("row realtime order = [%d %d], want [500000000 400000000]", result.Rows[0].RealtimeUsec, result.Rows[1].RealtimeUsec)
	}
	if result.Stats.RowsExamined != 2 {
		t.Fatalf("rows examined = %d, want 2", result.Stats.RowsExamined)
	}
}

func TestExplorerSamplingSkipsFieldExpansionAndRecordsHistogramEstimates(t *testing.T) {
	var entries []explorerTestEntry
	for i := 0; i < 200; i++ {
		entries = append(entries, explorerTestEntry{
			realtime: 1_000 + uint64(i),
			payloads: [][]byte{[]byte("PRIORITY=6"), []byte(fmt.Sprintf("MESSAGE=row-%03d", i))},
		})
	}
	path := createExplorerRawJournal(t, entries)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after, before := uint64(1_000), uint64(1_199)
	query := DefaultExplorerQuery().WithFacet([]byte("PRIORITY")).WithHistogram([]byte("PRIORITY"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.HistogramBuckets = 4
	query.Limit = 0
	query.Sampling = &ExplorerSampling{
		Budget:               4,
		MatchedFiles:         1,
		FileHeadRealtimeUsec: 1_000,
		FileTailRealtimeUsec: 1_199,
		FileHeadSeqnum:       1,
		FileTailSeqnum:       200,
		FileEntries:          200,
	}
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(sampling) error = %v", err)
	}
	if result.Stats.SamplingUnsampled == 0 && result.Stats.SamplingEstimated == 0 {
		t.Fatalf("sampling did not skip or estimate rows: %+v", result.Stats)
	}
	if result.Stats.DataPayloadsLoaded >= result.Stats.DataRefsSeen {
		t.Fatalf("sampling did not reduce payload loads: loaded=%d refs=%d", result.Stats.DataPayloadsLoaded, result.Stats.DataRefsSeen)
	}
	if explorerHistogramTotal(result, "[unsampled]")+explorerHistogramTotal(result, "[estimated]") == 0 {
		t.Fatalf("sampling histogram markers missing: %#v", result.Histogram)
	}
}

func TestExplorerFiltersWithOrValuesAndAndFields(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=3")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("a"), []byte("b")).
		WithFilter([]byte("PRIORITY"), []byte("3")).
		WithFacet([]byte("SERVICE"))
	query.Limit = 10

	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(or values and fields) error = %v", err)
	}
	if got := len(result.Rows); got != 2 {
		t.Fatalf("rows returned = %d, want 2", got)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "b", 1)
	if result.Stats.DataCacheMisses == 0 {
		t.Fatalf("data cache misses = 0, want positive")
	}
}

func TestExplorerFilteredTraversalSkipsUnmatchedEntriesBeforeControlCheck(t *testing.T) {
	entries := make([]explorerTestEntry, 0, 9_000)
	for i := 0; i < 8_999; i++ {
		entries = append(entries, explorerTestEntry{
			realtime: uint64(i + 1),
			payloads: [][]byte{[]byte("SERVICE=other"), []byte("FACET=ignore")},
		})
	}
	entries = append(entries, explorerTestEntry{
		realtime: 9_000,
		payloads: [][]byte{[]byte("SERVICE=target"), []byte("FACET=hit")},
	})
	path := createExplorerRawJournal(t, entries)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	control := NewExplorerControl()
	control.SetCancellationCallback(func() bool { return true })
	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("target")).
		WithFacet([]byte("FACET"))
	query.Limit = 0

	result, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyTraversal, control)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(filtered skip) error = %v", err)
	}
	if control.StopReason() != ExplorerStopNone {
		t.Fatalf("stop reason = %v, want none", control.StopReason())
	}
	assertExplorerFacetCount(t, result, "FACET", "hit", 1)
	if result.Stats.RowsExamined != 1 {
		t.Fatalf("rows examined = %d, want 1", result.Stats.RowsExamined)
	}
}

func TestExplorerReusesClassifiedDataObjects(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("PRIORITY=3")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("PRIORITY=3")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("PRIORITY"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(reuse data objects) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "PRIORITY", "3", 3)
	if result.Stats.DataCacheHits < 2 {
		t.Fatalf("data cache hits = %d, want at least 2", result.Stats.DataCacheHits)
	}
}

func TestExplorerGroupsFacetsWithSameFilterSet(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE")).WithFacet([]byte("PRIORITY"))
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(grouped facets) error = %v", err)
	}
	if result.Stats.RowsExamined != 2 || result.Stats.FacetRowsMatched != 2 {
		t.Fatalf("stats = %+v, want rows_examined=2 facet_rows_matched=2", result.Stats)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "PRIORITY", "4", 1)
}

func TestExplorerCombinesRowsHistogramAndFacetsInOnePass(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFacet([]byte("SERVICE")).
		WithHistogram([]byte("PRIORITY"))
	query.HistogramBuckets = 2
	query.Limit = 2
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(combined pass) error = %v", err)
	}
	if len(result.Rows) != 2 {
		t.Fatalf("rows returned = %d, want 2", len(result.Rows))
	}
	if result.Stats.RowsExamined != 2 || result.Stats.RowsMatched != 2 || result.Stats.FacetRowsMatched != 2 {
		t.Fatalf("stats = %+v, want rows_examined=2 rows_matched=2 facet_rows_matched=2", result.Stats)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	if got := explorerHistogramTotal(result, "3") + explorerHistogramTotal(result, "4"); got != 2 {
		t.Fatalf("histogram total = %d, want 2", got)
	}
}

func TestExplorerSamplingUsesActualHistogramBucketCount(t *testing.T) {
	after, before := uint64(1_733_494_460_000_000), uint64(1_735_656_412_000_000)
	query := DefaultExplorerQuery().WithHistogram([]byte("PRIORITY"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.HistogramBuckets = 300
	query.Sampling = &ExplorerSampling{
		Budget:               20_000,
		MatchedFiles:         200,
		FileHeadRealtimeUsec: after,
		FileTailRealtimeUsec: before,
		FileHeadSeqnum:       1,
		FileTailSeqnum:       2,
		FileEntries:          2,
	}

	histogram := newExplorerHistogram([]byte("PRIORITY"), query)
	sampling := newExplorerSamplingState(query, len(histogram.Buckets))
	if sampling == nil {
		t.Fatal("sampling state is nil")
	}
	if got := len(histogram.Buckets); got != 302 {
		t.Fatalf("histogram bucket count = %d, want 302", got)
	}
	if got := len(sampling.perSlotSampled); got != len(histogram.Buckets) {
		t.Fatalf("sampling slots = %d, want %d", got, len(histogram.Buckets))
	}
}

func TestExplorerSamplingSeqnumEstimateClampsOverScannedToOne(t *testing.T) {
	after, before := uint64(1), uint64(100)
	query := DefaultExplorerQuery()
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.Direction = DirectionForward
	query.Sampling = &ExplorerSampling{
		Budget:               20,
		MatchedFiles:         1,
		FileHeadRealtimeUsec: 1,
		FileTailRealtimeUsec: 100,
		FileHeadSeqnum:       1,
		FileTailSeqnum:       100,
		FileEntries:          3,
	}
	sampling := newExplorerSamplingState(query, 0)
	if sampling == nil {
		t.Fatal("sampling state is nil")
	}
	sampling.perFileSampled = 10
	remaining, ok := sampling.estimateRemainingRowsBySeqnum(5)
	if !ok {
		t.Fatal("estimateRemainingRowsBySeqnum() not available")
	}
	if remaining != 1 {
		t.Fatalf("remaining rows = %d, want 1", remaining)
	}
}

func TestExplorerEstimatedHistogramDistributionMatchesNetdataIntegerMath(t *testing.T) {
	histogram := &ExplorerHistogram{
		Field: []byte("PRIORITY"),
		Buckets: []ExplorerHistogramBucket{
			{StartRealtimeUsec: 0, EndRealtimeUsec: 10, Values: make(map[string]uint64)},
			{StartRealtimeUsec: 10, EndRealtimeUsec: 20, Values: make(map[string]uint64)},
			{StartRealtimeUsec: 20, EndRealtimeUsec: 30, Values: make(map[string]uint64)},
		},
	}
	var stats ExplorerStats
	addEstimatedHistogramRange(histogram, 0, 30, 10, &stats)

	counts := []uint64{
		histogram.Buckets[0].Values[string(explorerEstimatedValue)],
		histogram.Buckets[1].Values[string(explorerEstimatedValue)],
		histogram.Buckets[2].Values[string(explorerEstimatedValue)],
	}
	if fmt.Sprint(counts) != "[3 3 3]" {
		t.Fatalf("estimated histogram counts = %v, want [3 3 3]", counts)
	}
	if got := counts[0] + counts[1] + counts[2]; got != 9 {
		t.Fatalf("estimated histogram total = %d, want 9", got)
	}
}

func TestExplorerFiltersThenCombinesOutputsInOneCandidatePass(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=c"), []byte("PRIORITY=3")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("PRIORITY"), []byte("3")).
		WithFacet([]byte("SERVICE")).
		WithHistogram([]byte("SERVICE"))
	query.HistogramBuckets = 2
	query.Limit = 10
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(filtered combined pass) error = %v", err)
	}
	if len(result.Rows) != 2 {
		t.Fatalf("rows returned = %d, want 2", len(result.Rows))
	}
	if result.Stats.RowsExamined != 2 || result.Stats.RowsMatched != 2 || result.Stats.FacetRowsMatched != 2 {
		t.Fatalf("stats = %+v, want rows_examined=2 rows_matched=2 facet_rows_matched=2", result.Stats)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "c", 1)
	if got := result.Facets["SERVICE"]["b"]; got != 0 {
		t.Fatalf("SERVICE=b count = %d, want absent/0", got)
	}
}

func TestExplorerCursorRowsDeferPayloadExpansion(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3"), []byte("MESSAGE=hello")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery()
	query.Limit = 1
	result, err := reader.exploreCursorRows(query, ExplorerStrategyTraversal, nil)
	if err != nil {
		t.Fatalf("exploreCursorRows() error = %v", err)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows returned = %d, want 1", len(result.Rows))
	}
	if result.Rows[0].Cursor == "" {
		t.Fatal("cursor-only row has empty cursor")
	}
	if len(result.Rows[0].Payloads) != 0 {
		t.Fatalf("cursor-only payloads = %d, want 0", len(result.Rows[0].Payloads))
	}
	if result.Stats.ReturnedRowExpansions != 0 {
		t.Fatalf("returned row expansions = %d, want 0", result.Stats.ReturnedRowExpansions)
	}
}

func TestExplorerIndexCursorRowsDeferPayloadExpansion(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3"), []byte("MESSAGE=hello")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4"), []byte("MESSAGE=world")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFilter([]byte("PRIORITY"), []byte("3"))
	query.Limit = 1
	query.FieldMode = ExplorerFieldModeAllValues
	query.UseSourceRealtime = false
	result, err := reader.exploreCursorRows(query, ExplorerStrategyIndex, nil)
	if err != nil {
		t.Fatalf("exploreCursorRows(index) error = %v", err)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows returned = %d, want 1", len(result.Rows))
	}
	if result.Rows[0].Cursor == "" {
		t.Fatal("index cursor-only row has empty cursor")
	}
	if len(result.Rows[0].Payloads) != 0 {
		t.Fatalf("index cursor-only payloads = %d, want 0", len(result.Rows[0].Payloads))
	}
	if result.Stats.ReturnedRowExpansions != 0 {
		t.Fatalf("returned row expansions = %d, want 0", result.Stats.ReturnedRowExpansions)
	}
	if result.Stats.RowsExamined != 1 || result.Stats.RowsMatched != 1 {
		t.Fatalf("stats = %+v, want one indexed candidate row examined and matched", result.Stats)
	}
}

func TestExplorerIndexStrategyMatchesTraversalForAllValues(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3"), []byte("TAG=x")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=3"), []byte("TAG=x")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=4"), []byte("TAG=y"), []byte("TAG=z")}},
		{realtime: 4_000, payloads: [][]byte{[]byte("PRIORITY=3")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after, before := uint64(0), uint64(5_000)
	query := DefaultExplorerQuery().
		WithFilter([]byte("PRIORITY"), []byte("3")).
		WithFacet([]byte("SERVICE")).
		WithFacet([]byte("TAG")).
		WithHistogram([]byte("SERVICE"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.HistogramBuckets = 2
	query.Limit = 2
	query.FieldMode = ExplorerFieldModeAllValues
	query.UseSourceRealtime = false

	result, err := reader.ExploreWithStrategy(query, ExplorerStrategyIndex)
	if err != nil {
		t.Fatalf("ExploreWithStrategy(index) error = %v", err)
	}
	if len(result.Rows) != 2 {
		t.Fatalf("rows returned = %d, want 2", len(result.Rows))
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "b", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "-", 1)
	if got := explorerHistogramTotal(result, "a") + explorerHistogramTotal(result, "b") + explorerHistogramTotal(result, "-"); got != 3 {
		t.Fatalf("histogram total = %d, want 3", got)
	}
}

func TestExplorerIndexStrategyPreservesSameFieldFilterExclusion(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=3")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFilter([]byte("SERVICE"), []byte("a")).
		WithFilter([]byte("PRIORITY"), []byte("3")).
		WithFacet([]byte("SERVICE")).
		WithFacet([]byte("PRIORITY"))
	query.FieldMode = ExplorerFieldModeAllValues
	query.UseSourceRealtime = false
	result, err := reader.ExploreWithStrategy(query, ExplorerStrategyIndex)
	if err != nil {
		t.Fatalf("ExploreWithStrategy(index same-field exclusion) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	assertExplorerFacetCount(t, result, "SERVICE", "b", 1)
	assertExplorerFacetCount(t, result, "PRIORITY", "3", 1)
	assertExplorerFacetCount(t, result, "PRIORITY", "4", 1)
}

func TestExplorerIndexStrategyRejectsFirstValueSemantics(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("TAG=one"), []byte("TAG=two")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("TAG"))
	query.FieldMode = ExplorerFieldModeFirstValue
	_, err := reader.ExploreWithStrategy(query, ExplorerStrategyIndex)
	if !errors.Is(err, ErrUnsupported) {
		t.Fatalf("ExploreWithStrategy(index first-value) error = %v, want %v", err, ErrUnsupported)
	}
}

func TestExplorerRejectsUnsupportedStrategy(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	_, err := reader.ExploreWithStrategy(DefaultExplorerQuery(), ExplorerStrategy(99))
	if !errors.Is(err, ErrUnsupported) {
		t.Fatalf("ExploreWithStrategy(unsupported) error = %v, want %v", err, ErrUnsupported)
	}
}

func TestExplorerFirstValueDoesNotDoubleCountDuplicateFacetsOrHistogram(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{
			[]byte("_SOURCE_REALTIME_TIMESTAMP=1000"),
			[]byte("TAG=one"),
			[]byte("TAG=two"),
			[]byte("MESSAGE=after-tag"),
		}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("TAG")).WithHistogram([]byte("TAG"))
	query.HistogramBuckets = 1
	query.Limit = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(first-value no double count) error = %v", err)
	}
	if got := explorerFacetTotal(result, "TAG"); got != 1 {
		t.Fatalf("first-value TAG facet total = %d, want 1", got)
	}
	if got := explorerHistogramTotalAllValues(result); got != 1 {
		t.Fatalf("first-value histogram total = %d, want 1", got)
	}

	reader = mustOpenReaderFile(t, path)
	defer reader.Close()
	query.FieldMode = ExplorerFieldModeAllValues
	allValues, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(all-values no double count) error = %v", err)
	}
	if got := explorerFacetTotal(allValues, "TAG"); got != 2 {
		t.Fatalf("all-values TAG facet total = %d, want 2", got)
	}
	if got := explorerHistogramTotalAllValues(allValues); got != 2 {
		t.Fatalf("all-values histogram total = %d, want 2", got)
	}
}

func TestExplorerFirstValueTracksRequiredFieldIdentities(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("TAG=one"), []byte("TAG=two"), []byte("SERVICE=a")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("TAG")).WithFacet([]byte("SERVICE"))
	query.Limit = 0
	query.FieldMode = ExplorerFieldModeFirstValue
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(first-value identities) error = %v", err)
	}
	if got := explorerFacetTotal(result, "TAG"); got != 1 {
		t.Fatalf("TAG facet total = %d, want 1", got)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "a", 1)
	if result.Stats.EarlyStops != 1 {
		t.Fatalf("early stops = %d, want 1", result.Stats.EarlyStops)
	}
}

func TestExplorerRejectsDuplicateFacetFields(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE")).WithFacet([]byte("SERVICE"))
	query.Limit = 0
	_, err := reader.Explore(query)
	if !errors.Is(err, errInvalidJournal) {
		t.Fatalf("Explore(duplicate facets) error = %v, want %v", err, errInvalidJournal)
	}
}

func TestExplorerEmptyResultKeepsRequestedFacetWithNoValues(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after, before := uint64(10_000), uint64(20_000)
	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.Limit = 10
	query.RealtimeSlackUsec = 0
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(empty result) error = %v", err)
	}
	if len(result.Rows) != 0 || result.Stats.RowsMatched != 0 {
		t.Fatalf("result rows=%d rows_matched=%d, want 0/0", len(result.Rows), result.Stats.RowsMatched)
	}
	values, ok := result.Facets["SERVICE"]
	if !ok {
		t.Fatalf("SERVICE facet missing from %#v", result.Facets)
	}
	if len(values) != 0 {
		t.Fatalf("SERVICE facet values = %#v, want empty", values)
	}
}

func TestExplorerFacetTimeBoundsDoNotCountSlackRowsWithoutSourceRealtime(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 340_000_000, payloads: [][]byte{[]byte("SERVICE=before")}},
		{realtime: 360_000_000, payloads: [][]byte{[]byte("SERVICE=inside")}},
		{realtime: 400_000_000, payloads: [][]byte{[]byte("SERVICE=after")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after, before := uint64(350_000_000), uint64(370_000_000)
	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.Limit = 0
	query.RealtimeSlackUsec = 20_000_000
	query.UseSourceRealtime = false
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(facet bounds) error = %v", err)
	}
	assertExplorerFacetCount(t, result, "SERVICE", "inside", 1)
	if result.Facets["SERVICE"]["before"] != 0 || result.Facets["SERVICE"]["after"] != 0 {
		t.Fatalf("slack rows counted in facets: %#v", result.Facets["SERVICE"])
	}
	if result.Stats.FacetRowsMatched != 1 {
		t.Fatalf("facet rows matched = %d, want 1", result.Stats.FacetRowsMatched)
	}
}

func TestExplorerFtsOrTermsAndNegativeTermsFilterRows(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("TAG=alpha"), []byte("MESSAGE=alpha keep")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("TAG=beta"), []byte("MESSAGE=beta keep")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("TAG=debug"), []byte("MESSAGE=alpha debug")}},
		{realtime: 4_000, payloads: [][]byte{[]byte("TAG=other"), []byte("MESSAGE=other")}},
		{realtime: 5_000, payloads: [][]byte{[]byte("TAG=wild"), []byte("MESSAGE=start middle end")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().
		WithFacet([]byte("TAG")).
		WithFTSPattern([]byte("alpha")).
		WithFTSPattern([]byte("beta")).
		WithFTSNegativePattern([]byte("debug")).
		WithFTSPattern([]byte("start*end"))
	query.Limit = 10
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(fts or negative) error = %v", err)
	}
	if len(result.Rows) != 3 {
		t.Fatalf("rows returned = %d, want 3", len(result.Rows))
	}
	assertExplorerFacetCount(t, result, "TAG", "alpha", 1)
	assertExplorerFacetCount(t, result, "TAG", "beta", 1)
	assertExplorerFacetCount(t, result, "TAG", "wild", 1)
	if result.Facets["TAG"]["debug"] != 0 || result.Facets["TAG"]["other"] != 0 {
		t.Fatalf("excluded FTS rows counted: %#v", result.Facets["TAG"])
	}
}

func TestExplorerAutoAnchorScansBackwardFromTail(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery()
	query.Direction = DirectionBackward
	query.Limit = 2
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(backward auto anchor) error = %v", err)
	}
	if len(result.Rows) != 2 {
		t.Fatalf("rows returned = %d, want 2", len(result.Rows))
	}
	if result.Rows[0].RealtimeUsec != 2_000 || result.Rows[1].RealtimeUsec != 1_000 {
		t.Fatalf("row realtime order = [%d %d], want [2000 1000]", result.Rows[0].RealtimeUsec, result.Rows[1].RealtimeUsec)
	}
}

func TestExplorerRealtimeAnchor(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("SERVICE=a")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("SERVICE=b")}},
		{realtime: 3_000, payloads: [][]byte{[]byte("SERVICE=c")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery()
	query.Anchor = RealtimeExplorerAnchor(2_000)
	query.Limit = 2
	query.StopWhenRowsFull = true
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(realtime anchor) error = %v", err)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows returned = %d, want 1", len(result.Rows))
	}
	if result.Rows[0].RealtimeUsec != 3_000 {
		t.Fatalf("row realtime = %d, want 3000", result.Rows[0].RealtimeUsec)
	}
}

func TestExplorerHistogramAndFTSAreOptIn(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("MESSAGE=alpha"), []byte("PRIORITY=3")}},
		{realtime: 2_000, payloads: [][]byte{[]byte("MESSAGE=beta"), []byte("PRIORITY=4")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	after, before := uint64(0), uint64(3_000)
	query := DefaultExplorerQuery().
		WithHistogram([]byte("PRIORITY")).
		WithFTSPattern([]byte("alp"))
	query.AfterRealtimeUsec = &after
	query.BeforeRealtimeUsec = &before
	query.HistogramBuckets = 2
	query.Limit = 10
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(histogram fts opt-in) error = %v", err)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows returned = %d, want 1", len(result.Rows))
	}
	if result.Stats.FTSScans == 0 {
		t.Fatalf("fts scans = 0, want positive")
	}
	if got := explorerHistogramTotalAllValues(result); got != 1 {
		t.Fatalf("histogram total = %d, want 1", got)
	}
}

func TestExplorerFirstValueStopsAfterSameDataSatisfiesMultipleRoles(t *testing.T) {
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: 1_000, payloads: [][]byte{[]byte("_SOURCE_REALTIME_TIMESTAMP=1000"), []byte("MESSAGE=after-source")}},
	})
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithHistogram(sourceRealtimeField)
	query.HistogramBuckets = 1
	query.Limit = 0
	query.FieldMode = ExplorerFieldModeFirstValue
	result, err := reader.Explore(query)
	if err != nil {
		t.Fatalf("Explore(same data roles) error = %v", err)
	}
	if result.Stats.HistogramUpdates != 1 {
		t.Fatalf("histogram updates = %d, want 1", result.Stats.HistogramUpdates)
	}
	if result.Stats.EarlyStops != 1 {
		t.Fatalf("early stops = %d, want 1", result.Stats.EarlyStops)
	}
	if got := explorerHistogramTotalAllValues(result); got != 1 {
		t.Fatalf("histogram total = %d, want 1", got)
	}
}

func TestExplorerControlIndexStrategyReportsProgress(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	var reports []uint64
	control := NewExplorerControl()
	control.SetProgressInterval(0)
	control.SetProgressCallback(func(progress ExplorerProgress) {
		reports = append(reports, progress.Stats.RowsExamined)
	})
	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.Limit = 9_000
	query.FieldMode = ExplorerFieldModeAllValues
	query.UseSourceRealtime = false
	result, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyIndex, control)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(index) error = %v", err)
	}
	if result.Stats.RowsReturned != 9_000 {
		t.Fatalf("rows returned = %d, want 9000", result.Stats.RowsReturned)
	}
	if len(reports) == 0 {
		t.Fatal("progress callback was not called")
	}
}

func TestExplorerStopWhenRowsFull(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)
	reader := mustOpenReaderFile(t, path)
	defer reader.Close()

	query := DefaultExplorerQuery().WithFacet([]byte("SERVICE"))
	query.Limit = 1
	query.StopWhenRowsFull = true
	query.StopWhenRowsFullEvery = 1
	query.RealtimeSlackUsec = 0
	control := NewExplorerControl()
	result, err := reader.ExploreWithStrategyAndControl(query, ExplorerStrategyTraversal, control)
	if err != nil {
		t.Fatalf("ExploreWithStrategyAndControl(stop when rows full) error = %v", err)
	}
	if len(result.Rows) != 1 {
		t.Fatalf("rows returned = %d, want 1", len(result.Rows))
	}
	if result.Stats.RowsExamined >= 9_000 {
		t.Fatalf("rows examined = %d, want less than full scan", result.Stats.RowsExamined)
	}
}

type explorerTestEntry struct {
	realtime uint64
	payloads [][]byte
}

func createExplorerRawJournal(t *testing.T, entries []explorerTestEntry) string {
	return createExplorerRawJournalWithOptions(t, entries, nil)
}

func createExplorerRawJournalWithOptions(t *testing.T, entries []explorerTestEntry, configure func(*Options)) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "explorer.journal")
	opts := testOptions()
	opts.FieldNamePolicy = FieldNamePolicyRaw
	if configure != nil {
		configure(&opts)
	}
	writer, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	for index, entry := range entries {
		if err := writer.AppendRaw(entry.payloads, EntryOptions{RealtimeUsec: entry.realtime, MonotonicUsec: uint64(index + 1)}); err != nil {
			t.Fatalf("AppendRaw(%d) error = %v", index, err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	return path
}

func createExplorerManyJournal(t *testing.T, count int) string {
	t.Helper()
	entries := make([]explorerTestEntry, 0, count)
	for index := 0; index < count; index++ {
		service := "odd"
		if index%2 == 0 {
			service = "even"
		}
		entries = append(entries, explorerTestEntry{
			realtime: 1_700_000_000_000_000 + uint64(index),
			payloads: [][]byte{
				[]byte(fmt.Sprintf("MESSAGE=row-%d", index)),
				[]byte("SERVICE=" + service),
			},
		})
	}
	return createExplorerRawJournal(t, entries)
}

func assertExplorerFacetCount(t *testing.T, result ExplorerResult, field, value string, want uint64) {
	t.Helper()
	values, ok := result.Facets[field]
	if !ok {
		t.Fatalf("facet %s missing from %#v", field, result.Facets)
	}
	if got := values[value]; got != want {
		t.Fatalf("facet %s=%s count = %d, want %d (values=%#v)", field, value, got, want, values)
	}
}

func explorerHistogramTotal(result ExplorerResult, value string) uint64 {
	if result.Histogram == nil {
		return 0
	}
	var total uint64
	for _, bucket := range result.Histogram.Buckets {
		total += bucket.Values[value]
	}
	return total
}

func explorerHistogramTotalAllValues(result ExplorerResult) uint64 {
	if result.Histogram == nil {
		return 0
	}
	var total uint64
	for _, bucket := range result.Histogram.Buckets {
		for _, count := range bucket.Values {
			total += count
		}
	}
	return total
}

func explorerFacetTotal(result ExplorerResult, field string) uint64 {
	var total uint64
	for _, count := range result.Facets[field] {
		total += count
	}
	return total
}
