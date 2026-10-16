package journal

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"
)

type explorerWindowCase struct {
	Path        string
	Kind        string
	Mode        ReaderAccessMode
	Compression int
	TimeRange   bool
	ExpectError bool
}

const explorerWindowMarker = "JOURNAL_SDK_EXPLORER_WINDOW_CASE"

func runExplorerWindowChild(t *testing.T, c explorerWindowCase) {
	t.Helper()
	raw, err := json.Marshal(c)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	// Isolate a possible mmap fault so every backend/layout still runs.
	cmd := exec.CommandContext(ctx, os.Args[0], "-test.run=^TestExplorerIndexedWindowLifetime$", "-test.count=1")
	cmd.Env = append(os.Environ(), explorerWindowMarker+"="+string(raw))
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("child failed: %v\n%s", err, output)
	}
}

func explorerWindowFixture(t testing.TB, compact bool, compression, rows, cardinality int) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "explorer.journal")
	opts := testOptions()
	opts.Compact = compact
	opts.Compression = compression
	opts.CompressThresholdBytes = 32
	cadence := uint64(0)
	opts.LivePublishEveryEntries = &cadence
	w, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < rows; i++ {
		selected := "skip"
		if i%64 == 0 {
			selected = "keep"
		}
		fields := []Field{StringField("SELECT", selected), StringField("MESSAGE", fmt.Sprintf("row-%04d", i))}
		if i%5 != 0 {
			fields = append(fields, StringField("VALUE", fmt.Sprintf("%04d-%s", i%cardinality, strings.Repeat("value-", 96))))
		}
		if i%11 == 0 && i%5 != 0 {
			fields = append(fields, StringField("VALUE", "extra-"+strings.Repeat("more-", 96)))
		}
		if err := w.Append(fields, EntryOptions{RealtimeUsec: 1000 + uint64(i), MonotonicUsec: 1 + uint64(i)}); err != nil {
			_ = w.Close()
			t.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	return path
}

func explorerWindowQuery(kind string, sparse bool) ExplorerQuery {
	q := DefaultExplorerQuery()
	q.FieldMode = ExplorerFieldModeAllValues
	q.Direction = DirectionForward
	q.UseSourceRealtime = false
	q.Limit = 3
	q.HistogramBuckets = 4
	if sparse {
		q = q.WithFilter([]byte("SELECT"), []byte("keep"))
	}
	if kind == "facet" {
		q = q.WithFacet([]byte("VALUE"))
	} else {
		q = q.WithHistogram([]byte("VALUE"))
	}
	return q
}

func TestExplorerIndexedWindowLifetime(t *testing.T) {
	if raw := os.Getenv(explorerWindowMarker); raw != "" {
		var c explorerWindowCase
		if err := json.Unmarshal([]byte(raw), &c); err != nil {
			t.Fatal(err)
		}
		query := explorerWindowQuery(c.Kind, true)
		if c.TimeRange {
			after, before := uint64(1100), uint64(1800)
			query.AfterRealtimeUsec, query.BeforeRealtimeUsec = &after, &before
		}
		if c.ExpectError {
			query.Filters = nil
			query.Limit = 0
		}
		control, err := OpenFile(c.Path)
		if err != nil {
			t.Fatal(err)
		}
		want, err := control.ExploreWithStrategy(query, ExplorerStrategyTraversal)
		_ = control.Close()
		if err != nil {
			t.Fatal(err)
		}
		control, err = OpenFile(c.Path)
		if err != nil {
			t.Fatal(err)
		}
		wantIndex, indexErr := control.ExploreWithStrategy(query, ExplorerStrategyIndex)
		_ = control.Close()
		expectedError := errInvalidJournal
		if errors.Is(indexErr, errCorruptObject) {
			expectedError = errCorruptObject
		}
		if c.ExpectError && !errors.Is(indexErr, expectedError) {
			t.Fatalf("wide-window malformed index error = %v", indexErr)
		}
		if !c.ExpectError && indexErr != nil {
			t.Fatal(indexErr)
		}
		window := uint64(4096)
		if c.Mode == ReaderAccessReadAt {
			window = 64
		}
		r, err := OpenFileWithOptions(c.Path, DefaultReaderOptions().WithAccessMode(c.Mode).WithWindowSize(window).WithMaxWindows(1))
		if err != nil {
			t.Fatal(err)
		}
		defer r.Close()
		before := r.AccessStats()
		got, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
		if c.ExpectError {
			if !errors.Is(err, expectedError) {
				t.Fatalf("malformed index error = %v", err)
			}
			return
		}
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got.Facets, want.Facets) || !reflect.DeepEqual(got.Histogram, want.Histogram) || !reflect.DeepEqual(got.Rows, want.Rows) {
			t.Fatalf("indexed/traversal mismatch: facet=%v histogram=%v rows=%v", reflect.DeepEqual(got.Facets, want.Facets), reflect.DeepEqual(got.Histogram, want.Histogram), reflect.DeepEqual(got.Rows, want.Rows))
		}
		if got.Stats.FacetUpdates != want.Stats.FacetUpdates || got.Stats.HistogramUpdates != want.Stats.HistogramUpdates {
			t.Fatalf("update counts differ: indexed=%+v traversal=%+v", got.Stats, want.Stats)
		}
		if got.Stats != wantIndex.Stats {
			t.Fatalf("indexed statistics differ across access bounds: got=%+v want=%+v", got.Stats, wantIndex.Stats)
		}
		if c.Compression != CompressionNone && got.Stats.PayloadsDecompressed == 0 {
			t.Fatal("fixture did not exercise compressed DATA")
		}
		after := r.AccessStats()
		if after.ActiveWindows > 1 || after.RowArenaPeakBytes > after.RowArenaLimitBytes {
			t.Fatalf("reader memory bound exceeded: %+v", after)
		}
		if after.SelectedAccessMode == ReaderAccessMmap && after.EvictionCount <= before.EvictionCount {
			t.Fatalf("fixture did not force window eviction: %+v", after)
		}
		if c.Mode == ReaderAccessReadAt && after.TempCopyCount <= before.TempCopyCount {
			t.Fatalf("fixture did not force temporary scratch reads: %+v", after)
		}
		return
	}
	for _, compact := range []bool{false, true} {
		for _, compression := range []int{CompressionNone, CompressionZSTD, CompressionXZ, CompressionLZ4} {
			path := explorerWindowFixture(t, compact, compression, 1024, 4)
			for _, kind := range []string{"facet", "histogram"} {
				for _, mode := range readerTestAccessModes() {
					t.Run(fmt.Sprintf("compact=%v/compression=%d/%s/%s", compact, compression, kind, accessModeName(mode)), func(t *testing.T) {
						runExplorerWindowChild(t, explorerWindowCase{Path: path, Kind: kind, Mode: mode, Compression: compression})
					})
				}
			}
		}
	}
}

func TestExplorerIndexedWindowValueShapes(t *testing.T) {
	for _, compact := range []bool{false, true} {
		entries := make([]explorerTestEntry, 1024)
		for i := range entries {
			value := ""
			if i%3 != 0 {
				value = "\x00\xff=" + strings.Repeat("binary-", 96)
			}
			if i < 100 || i > 800 {
				value = "outside-" + strings.Repeat("range-", 96)
			}
			selected := "skip"
			if i%64 == 0 {
				selected = "keep"
			}
			entries[i] = explorerTestEntry{realtime: 1000 + uint64(i), payloads: [][]byte{[]byte("VALUE=" + value), []byte("SELECT=" + selected), []byte(fmt.Sprintf("MESSAGE=row-%04d", i))}}
		}
		path := createExplorerRawJournalWithOptions(t, entries, func(opts *Options) { opts.Compact = compact })
		for _, kind := range []string{"facet", "histogram"} {
			for _, mode := range readerTestAccessModes() {
				t.Run(fmt.Sprintf("compact=%v/%s/%s", compact, kind, accessModeName(mode)), func(t *testing.T) {
					runExplorerWindowChild(t, explorerWindowCase{Path: path, Kind: kind, Mode: mode, TimeRange: true})
				})
			}
		}
	}
}

func TestExplorerIndexedWindowSingletonValues(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, compression := range []int{CompressionNone, CompressionZSTD, CompressionXZ, CompressionLZ4} {
			entries := make([]explorerTestEntry, 1024)
			for i := range entries {
				value := fmt.Sprintf("%04d\x00\xff=%s", i, strings.Repeat("singleton-", 64))
				if i == 64 {
					value = ""
				}
				selected := "skip"
				if i%64 == 0 {
					selected = "keep"
				}
				entries[i] = explorerTestEntry{realtime: 1000 + uint64(i), payloads: [][]byte{[]byte("VALUE=" + value), []byte("SELECT=" + selected), []byte(fmt.Sprintf("MESSAGE=row-%04d", i))}}
			}
			path := createExplorerRawJournalWithOptions(t, entries, func(opts *Options) {
				opts.Compact = compact
				opts.Compression = compression
				opts.CompressThresholdBytes = 32
			})
			for _, timeRange := range []bool{false, true} {
				for _, mode := range readerTestAccessModes() {
					t.Run(fmt.Sprintf("compact=%v/compression=%d/time=%v/%s", compact, compression, timeRange, accessModeName(mode)), func(t *testing.T) {
						runExplorerWindowChild(t, explorerWindowCase{Path: path, Kind: "histogram", Mode: mode, Compression: compression, TimeRange: timeRange})
					})
				}
			}
		}
	}
}

func TestExplorerIndexedWindowSingletonBounds(t *testing.T) {
	maxInt := uint64(^uint(0) >> 1)
	upperStart := ^uint64(0) - maxInt
	cases := []struct {
		name                 string
		realtime, start, end uint64
		buckets              int
		updates              uint64
	}{
		{"maximum-int-index", maxInt, 0, 1, 1, 1},
		{"negative-int-index", maxInt + 1, 0, 1, 1, 0},
		{"zero-width", 1234, 0, 0, 1, 1},
		{"below-start", 1000, upperStart, upperStart + 1, 1, 1},
		{"maximum-quotient", ^uint64(0), upperStart, upperStart + 1, 1, 1},
		{"no-buckets", 1000, 0, 1, 0, 0},
	}
	value := "\x00\xff=" + strings.Repeat("bounds-", 96)
	for _, compact := range []bool{false, true} {
		for _, c := range cases {
			t.Run(fmt.Sprintf("compact=%v/%s", compact, c.name), func(t *testing.T) {
				path := createExplorerRawJournalWithOptions(t, []explorerTestEntry{{realtime: c.realtime, payloads: [][]byte{[]byte("VALUE=" + value), []byte("PADDING=" + strings.Repeat("x", 8192))}}}, func(opts *Options) { opts.Compact = compact })
				r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithAccessMode(ReaderAccessReadAt).WithWindowSize(64).WithMaxWindows(1))
				if err != nil {
					t.Fatal(err)
				}
				defer r.Close()
				histogram := &ExplorerHistogram{Field: []byte("VALUE")}
				for i := 0; i < c.buckets; i++ {
					histogram.Buckets = append(histogram.Buckets, ExplorerHistogramBucket{StartRealtimeUsec: c.start, EndRealtimeUsec: c.end, Values: make(map[string]uint64)})
				}
				query := explorerWindowQuery("histogram", false)
				result := ExplorerResult{Histogram: histogram}
				if err := r.indexedCountHistogram(query, explorerCandidateSet{all: true, count: 1}, &result); err != nil {
					t.Fatal(err)
				}
				if result.Stats.DataRefsSeen != 1 || result.Stats.HistogramUpdates != c.updates {
					t.Fatalf("unexpected statistics: %+v", result.Stats)
				}
				var total uint64
				for _, bucket := range histogram.Buckets {
					for key, count := range bucket.Values {
						if key != value {
							t.Fatalf("singleton value changed: %q", key)
						}
						total += count
					}
				}
				if total != c.updates {
					t.Fatalf("bucket updates = %d, want %d", total, c.updates)
				}
			})
		}
	}
}

func TestExplorerIndexedWindowReadErrors(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, fault := range []string{"posting", "entry"} {
			path := explorerWindowFixture(t, compact, CompressionNone, 1024, 4)
			r := mustOpenReaderFile(t, path)
			offset, found, err := r.findDataOffsetByPayload([]byte("VALUE=0001-" + strings.Repeat("value-", 96)))
			_ = r.Close()
			if err != nil || !found {
				t.Fatalf("find fixture DATA: found=%v err=%v", found, err)
			}
			pointer := offset + 48
			if fault == "entry" {
				pointer = offset + 40
			}
			// Point at a valid DATA object where an ENTRY_ARRAY or ENTRY is required.
			corruptHeaderUint64(t, path, int64(pointer), offset)
			for _, kind := range []string{"facet", "histogram"} {
				if fault == "entry" && kind != "histogram" {
					continue
				}
				for _, mode := range readerTestAccessModes() {
					t.Run(fmt.Sprintf("compact=%v/%s/%s/%s", compact, fault, kind, accessModeName(mode)), func(t *testing.T) {
						runExplorerWindowChild(t, explorerWindowCase{Path: path, Kind: kind, Mode: mode, ExpectError: true})
					})
				}
			}
		}
	}
}

func TestExplorerIndexedWindowPartialFacetStats(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, prefix := range []bool{false, true} {
			entries := []explorerTestEntry{
				{realtime: 1000, payloads: [][]byte{[]byte("VALUE=shared")}},
				{realtime: 2000, payloads: [][]byte{[]byte("VALUE=shared")}},
			}
			if prefix {
				// FIELD chains prepend new DATA, so append the successful control last.
				entries = append(entries, explorerTestEntry{realtime: 3000, payloads: [][]byte{[]byte("VALUE=first")}})
			}
			path := createExplorerRawJournalWithOptions(t, entries, func(opts *Options) { opts.Compact = compact })
			r := mustOpenReaderFile(t, path)
			offset, found, err := r.findDataOffsetByPayload([]byte("VALUE=shared"))
			if err == nil && found && prefix {
				first, firstFound, firstErr := r.findDataOffsetByPayload([]byte("VALUE=first"))
				head, headFound, headErr := r.findFieldHeadDataOffset([]byte("VALUE"))
				if firstErr != nil || headErr != nil || !firstFound || !headFound || head != first {
					_ = r.Close()
					t.Fatalf("successful DATA must precede failed walk: first=%d head=%d errors=%v/%v", first, head, firstErr, headErr)
				}
			}
			_ = r.Close()
			if err != nil || !found {
				t.Fatalf("find fixture DATA: found=%v err=%v", found, err)
			}
			corruptHeaderUint64(t, path, int64(offset+48), offset)
			for _, all := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/prefix=%v/all=%v", compact, prefix, all), func(t *testing.T) {
					r := mustOpenReaderFile(t, path)
					defer r.Close()
					selectedOffset := r.entryOffsets[0]
					if prefix {
						selectedOffset = r.entryOffsets[len(r.entryOffsets)-1]
					}
					candidates := explorerCandidateSet{all: all, count: 1, offsets: map[uint64]struct{}{selectedOffset: {}}}
					if all {
						candidates.count = uint64(len(r.entryOffsets))
					}
					result := ExplorerResult{Facets: make(map[string]map[string]uint64)}
					err := r.indexedCountFacetGroup(explorerWindowQuery("facet", false), facetPassGroup{facetIndices: []int{0}}, candidates, &result)
					refs, updates, classified := uint64(1), uint64(0), uint64(0)
					if prefix {
						refs, updates, classified = 2, 1, 1
					}
					if !errors.Is(err, errInvalidJournal) || result.Stats.DataRefsSeen != refs || result.Stats.FacetUpdates != updates || result.Stats.DataObjectsClassified != classified || result.Stats.DataPayloadsLoaded != classified {
						t.Fatalf("partial posting error/counts: err=%v stats=%+v", err, result.Stats)
					}
				})
			}
		}
	}
}

func TestExplorerIndexedWindowPartialHistogramStats(t *testing.T) {
	for _, compact := range []bool{false, true} {
		for _, compression := range []int{CompressionNone, CompressionLZ4} {
			payload := []byte("VALUE=" + strings.Repeat("shared-", 96))
			path := createExplorerRawJournalWithOptions(t, []explorerTestEntry{
				{realtime: 1000, payloads: [][]byte{payload}},
			}, func(opts *Options) {
				opts.Compact = compact
				opts.Compression = compression
				opts.CompressThresholdBytes = 32
			})
			r := mustOpenReaderFile(t, path)
			offset, found, err := r.findDataOffsetByPayload(payload)
			if err != nil || !found {
				_ = r.Close()
				t.Fatalf("find fixture DATA: found=%v err=%v", found, err)
			}
			header, err := r.readDataHeaderAt(offset)
			_ = r.Close()
			if err != nil || header.nEntries != 1 || (compression == CompressionLZ4 && header.object.flag&objectCompressedLZ4 == 0) {
				t.Fatalf("incorrect singleton fixture: header=%+v err=%v", header, err)
			}
			corruptHeaderUint64(t, path, int64(offset+40), offset)
			for _, all := range []bool{false, true} {
				t.Run(fmt.Sprintf("compact=%v/compression=%d/all=%v", compact, compression, all), func(t *testing.T) {
					r := mustOpenReaderFile(t, path)
					defer r.Close()
					query := explorerWindowQuery("histogram", false)
					histogram := newExplorerHistogram([]byte("VALUE"), query)
					result := ExplorerResult{Histogram: &histogram}
					candidates := explorerCandidateSet{all: all, count: 1, offsets: map[uint64]struct{}{offset: {}}}
					err := r.indexedCountHistogram(query, candidates, &result)
					if !errors.Is(err, errCorruptObject) || result.Stats.DataRefsSeen != 1 || result.Stats.HistogramUpdates != 0 || result.Stats.DataObjectsClassified != 0 || result.Stats.DataPayloadsLoaded != 0 || result.Stats.PayloadsDecompressed != 0 {
						t.Fatalf("partial ENTRY error/counts: err=%v stats=%+v", err, result.Stats)
					}
				})
			}
		}
	}
}

func TestExplorerIndexedWindowPayloadFieldMatch(t *testing.T) {
	cases := []struct {
		name, field, payload string
		wrongObject          bool
	}{
		{name: "wrong-name", field: "VALUE", payload: "OTHER=value"},
		{name: "wrong-boundary", field: "VALUE", payload: "VALUE?value"},
		{name: "early-delimiter", field: "VALUE", payload: "VA=UE=value"},
		{name: "empty-parsed-name", field: "VALUE", payload: "=ALUE=value"},
		{name: "requested-delimiter", field: "VA=UE", payload: "VA=UE=value"},
		{name: "requested-delimiter-wrong-object", field: "VA=UE", payload: "VA=UE=value", wrongObject: true},
	}
	for _, compact := range []bool{false, true} {
		for _, c := range cases {
			path := createExplorerRawJournalWithOptions(t, []explorerTestEntry{
				{realtime: 1000, payloads: [][]byte{[]byte("VALUE=value")}},
			}, func(opts *Options) {
				opts.Compact = compact
				opts.FieldHashTableBuckets = 1
			})
			r := mustOpenReaderFile(t, path)
			dataOffset, found, err := r.findDataOffsetByPayload([]byte("VALUE=value"))
			if err != nil || !found {
				_ = r.Close()
				t.Fatalf("find fixture DATA: found=%v err=%v", found, err)
			}
			payloadOffset := dataOffset + r.dataPayloadOffset()
			fieldHash := r.hash([]byte(c.field))
			bucket, err := r.readSlice(r.header.fieldHashTableOffset, hashItemSize)
			if err != nil {
				_ = r.Close()
				t.Fatal(err)
			}
			fieldOffset := parseHashItem(bucket).head
			_ = r.Close()
			if c.field != "VALUE" {
				corruptHeaderUint64(t, path, int64(fieldOffset+16), fieldHash)
			}
			if c.wrongObject {
				corruptHeaderUint64(t, path, int64(fieldOffset+32), fieldOffset)
			}
			file, err := os.OpenFile(path, os.O_RDWR, 0)
			if err != nil {
				t.Fatal(err)
			}
			for offset, data := range map[uint64][]byte{
				fieldOffset + fieldObjectHeaderSize: []byte(c.field),
				payloadOffset:                       []byte(c.payload),
			} {
				if n, err := file.WriteAt(data, int64(offset)); err != nil || n != len(data) {
					_ = file.Close()
					t.Fatalf("patch fixture bytes: n=%d err=%v", n, err)
				}
			}
			if err := file.Close(); err != nil {
				t.Fatal(err)
			}
			for _, kind := range []string{"facet", "histogram"} {
				t.Run(fmt.Sprintf("compact=%v/%s/%s", compact, c.name, kind), func(t *testing.T) {
					r := mustOpenReaderFile(t, path)
					defer r.Close()
					field := []byte(c.field)
					head, found, err := r.findFieldHeadDataOffset(field)
					wantHead := dataOffset
					if c.wrongObject {
						wantHead = fieldOffset
					}
					if err != nil || !found || head != wantHead {
						t.Fatalf("fixture FIELD was not found: head=%d found=%v err=%v", head, found, err)
					}
					query := explorerWindowQuery(kind, false)
					query.Facets, query.Histogram = nil, nil
					result := ExplorerResult{Facets: make(map[string]map[string]uint64)}
					candidates := explorerCandidateSet{all: true, count: 1}
					if kind == "facet" {
						query = query.WithFacet(field)
						err = r.indexedCountFacetGroup(query, facetPassGroup{facetIndices: []int{0}}, candidates, &result)
					} else {
						query = query.WithHistogram(field)
						histogram := newExplorerHistogram(field, query)
						result.Histogram = &histogram
						err = r.indexedCountHistogram(query, candidates, &result)
					}
					if c.wrongObject {
						if !errors.Is(err, errCorruptObject) || result.Stats.FacetUpdates+result.Stats.HistogramUpdates != 0 {
							t.Fatalf("native FIELD chain error/counts: err=%v stats=%+v", err, result.Stats)
						}
					} else if err != nil || result.Stats.FacetUpdates+result.Stats.HistogramUpdates != 1 {
						t.Fatalf("unset update: err=%v stats=%+v", err, result.Stats)
					}
					if result.Stats.DataRefsSeen != 0 || result.Stats.DataObjectsClassified != 0 || result.Stats.DataPayloadsLoaded != 0 || result.Stats.PayloadsDecompressed != 0 {
						t.Fatalf("mismatched payload was aggregated: %+v", result.Stats)
					}
				})
			}
		}
	}
}

func BenchmarkExplorerIndexedValueOwnership(b *testing.B) {
	for _, cardinality := range []int{4, 1024} {
		for _, kind := range []string{"facet", "histogram"} {
			for _, sparse := range []bool{false, true} {
				b.Run(fmt.Sprintf("cardinality=%d/%s/sparse=%s", cardinality, kind, strconv.FormatBool(sparse)), func(b *testing.B) {
					path := explorerWindowFixture(b, false, CompressionNone, 2048, cardinality)
					r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
					if err != nil {
						b.Fatal(err)
					}
					defer r.Close()
					q := explorerWindowQuery(kind, sparse)
					q.Limit = 0
					b.ReportAllocs()
					b.ResetTimer()
					for i := 0; i < b.N; i++ {
						result, err := r.ExploreWithStrategy(q, ExplorerStrategyIndex)
						if err != nil || result.Stats.FacetUpdates+result.Stats.HistogramUpdates == 0 {
							b.Fatalf("incomplete query: %v %+v", err, result.Stats)
						}
					}
					b.StopTimer()
				})
			}
		}
	}
}

func BenchmarkExplorerIndexedCompressedValueOwnership(b *testing.B) {
	corpus := os.Getenv("JOURNAL_SDK_EXPLORER_COMPRESSED_CORPUS")
	if corpus == "" {
		b.Skip("set JOURNAL_SDK_EXPLORER_COMPRESSED_CORPUS to a prebuilt fixture directory")
	}
	for _, layout := range []string{"regular", "compact"} {
		for _, codec := range []string{"zstd", "xz", "lz4"} {
			for _, shape := range []string{"low", "high", "singleton"} {
				for _, kind := range []string{"facet", "histogram"} {
					b.Run(fmt.Sprintf("%s/%s/%s/%s", layout, codec, shape, kind), func(b *testing.B) {
						path := filepath.Join(corpus, layout, codec, shape+".journal")
						r, err := OpenFileWithOptions(path, DefaultReaderOptions().WithSnapshot(true))
						if err != nil {
							b.Fatal(err)
						}
						defer r.Close()
						query := explorerWindowQuery(kind, shape != "low")
						query.Limit = 0
						indexed, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
						if err != nil {
							b.Fatal(err)
						}
						traversal, err := r.ExploreWithStrategy(query, ExplorerStrategyTraversal)
						if err != nil {
							b.Fatal(err)
						}
						if !reflect.DeepEqual(indexed.Facets, traversal.Facets) || !reflect.DeepEqual(indexed.Histogram, traversal.Histogram) || !reflect.DeepEqual(indexed.Rows, traversal.Rows) || !reflect.DeepEqual(indexed.ColumnFields, traversal.ColumnFields) {
							b.Fatal("indexed/traversal results differ")
						}
						updates := uint64(2197)
						if shape == "high" {
							updates = 34
						} else if shape == "singleton" {
							updates = 32
						}
						if indexed.Stats.FacetUpdates+indexed.Stats.HistogramUpdates != updates || indexed.Stats.PayloadsDecompressed == 0 {
							b.Fatalf("incorrect or uncompressed fixture: %+v", indexed.Stats)
						}
						if stats := r.AccessStats(); stats.EvictionCount != 0 || stats.TempCopyCount != 0 {
							b.Fatalf("prebuilt performance control must fit its accessor window: %+v", stats)
						}
						encoded, err := json.Marshal(indexed.Stats)
						if err != nil {
							b.Fatal(err)
						}
						b.Logf("indexed-stats %s", encoded)
						b.ReportAllocs()
						b.ResetTimer()
						for i := 0; i < b.N; i++ {
							result, err := r.ExploreWithStrategy(query, ExplorerStrategyIndex)
							if err != nil || result.Stats.FacetUpdates+result.Stats.HistogramUpdates != updates {
								b.Fatalf("incomplete query: %v %+v", err, result.Stats)
							}
						}
						b.StopTimer()
					})
				}
			}
		}
	}
}
