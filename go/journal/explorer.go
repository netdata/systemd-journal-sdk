package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"sort"

	"github.com/klauspost/compress/zstd"
	"github.com/pierrec/lz4/v4"
	"github.com/ulikunitz/xz"
)

// ExplorerFilterKind controls whether a filter includes or excludes matching
// FIELD=value DATA objects.
type ExplorerFilterKind int

const (
	ExplorerFilterIn ExplorerFilterKind = iota
	ExplorerFilterNotIn
)

// ExplorerFilter is an indexed journal filter. Values for the same field are
// ORed for ExplorerFilterIn and excluded as a set for ExplorerFilterNotIn.
type ExplorerFilter struct {
	Field  []byte
	Values [][]byte
	Kind   ExplorerFilterKind
}

// FieldIn builds a positive FIELD IN [values...] filter.
func FieldIn(field []byte, values ...[]byte) ExplorerFilter {
	return ExplorerFilter{Field: cloneBytes(field), Values: cloneByteSlices(values), Kind: ExplorerFilterIn}
}

// FieldNotIn builds a negative FIELD NOT IN [values...] filter.
func FieldNotIn(field []byte, values ...[]byte) ExplorerFilter {
	return ExplorerFilter{Field: cloneBytes(field), Values: cloneByteSlices(values), Kind: ExplorerFilterNotIn}
}

// ExplorerDisplayMode controls row field expansion.
type ExplorerDisplayMode int

const (
	ExplorerDisplayAll ExplorerDisplayMode = iota
	ExplorerDisplayNone
	ExplorerDisplayFields
)

// ExplorerDisplay controls which fields are materialized for returned rows.
type ExplorerDisplay struct {
	Mode   ExplorerDisplayMode
	Fields [][]byte
}

func DisplayNone() ExplorerDisplay {
	return ExplorerDisplay{Mode: ExplorerDisplayNone}
}

func DisplayAll() ExplorerDisplay {
	return ExplorerDisplay{Mode: ExplorerDisplayAll}
}

func DisplayFields(fields ...[]byte) ExplorerDisplay {
	return ExplorerDisplay{Mode: ExplorerDisplayFields, Fields: cloneByteSlices(fields)}
}

// ExplorerQuery describes the optimized log-explorer query path.
type ExplorerQuery struct {
	Filters           []ExplorerFilter
	Facets            [][]byte
	FullText          []byte
	Display           ExplorerDisplay
	Limit             *int
	Direction         Direction
	SinceRealtimeUsec *uint64
	UntilRealtimeUsec *uint64
}

// DefaultExplorerQuery returns the same user-facing defaults as the Rust SDK.
func DefaultExplorerQuery() ExplorerQuery {
	limit := 100
	return ExplorerQuery{
		Display:   DisplayAll(),
		Limit:     &limit,
		Direction: DirectionForward,
	}
}

// WithFullText marks the full-text query as explicitly set. An empty needle is
// valid and matches all entries, so nil alone cannot distinguish unset.
func (q ExplorerQuery) WithFullText(needle []byte) ExplorerQuery {
	if len(needle) == 0 {
		q.FullText = []byte{}
	} else {
		q.FullText = cloneBytes(needle)
	}
	return q
}

// WithDisplay sets the row display mode.
func (q ExplorerQuery) WithDisplay(display ExplorerDisplay) ExplorerQuery {
	q.Display = display
	return q
}

// ExplorerUniqueQuery describes an optimized unique-values query.
type ExplorerUniqueQuery struct {
	Field             []byte
	Filters           []ExplorerFilter
	Limit             *int
	Skip              int
	IncludeCounts     bool
	SinceRealtimeUsec *uint64
	UntilRealtimeUsec *uint64
}

// ExplorerQueryCounters records which indexed and materializing paths a query
// used. It is intentionally part of the API so callers can prove fast paths.
type ExplorerQueryCounters struct {
	EntryOffsetsIndexed       uint64
	FilterDataObjectsExamined uint64
	CandidateEntries          uint64
	CandidateDataRefsVisited  uint64
	PayloadsMaterialized      uint64
	// PayloadsDecompressed counts selected payload decompressions during
	// facet/display/FTS/unique materialization. Internal decompression for DATA
	// hash-collision checks during filter planning is outside this diagnostic.
	PayloadsDecompressed    uint64
	FacetValuesMaterialized uint64
	FTSPayloadsScanned      uint64
	DisplayRowsExpanded     uint64
	ConstrainedFacetCounts  uint64
	FieldLinkageHits        uint64
	FieldLinkageFallbacks   uint64
}

type ExplorerRow struct {
	Realtime uint64
	Seqnum   uint64
	Cursor   string
	Fields   []RawField
}

type ExplorerFacetValue struct {
	Value []byte
	Count uint64
}

type ExplorerFacet struct {
	Field  []byte
	Values []ExplorerFacetValue
}

type ExplorerQueryResult struct {
	Rows            []ExplorerRow
	Facets          []ExplorerFacet
	TotalCandidates uint64
	Counters        ExplorerQueryCounters
}

type explorerQueryResultWithKeys struct {
	result  ExplorerQueryResult
	rowKeys []directoryEntryKey
}

type explorerRowWithKey struct {
	row ExplorerRow
	key directoryEntryKey
}

type ExplorerUniqueValue struct {
	Value []byte
	Count *uint64
}

type ExplorerUniqueResult struct {
	Field                 []byte
	Values                []ExplorerUniqueValue
	TotalValuesConsidered uint64
	Counters              ExplorerQueryCounters
}

type EntryDataRef struct {
	Offset     uint64
	Compressed bool
	PayloadLen uint64
}

type fieldDataMap struct {
	fields  [][]byte
	offsets map[uint64]int
}

func cloneByteSlices(values [][]byte) [][]byte {
	out := make([][]byte, 0, len(values))
	for _, value := range values {
		out = append(out, cloneBytes(value))
	}
	return out
}

func Limit(n int) *int {
	return &n
}

func RealtimeUsec(usec uint64) *uint64 {
	return &usec
}

// VisitEntryDataRefs reports DATA object references for the current entry
// without materializing payloads.
func (r *Reader) VisitEntryDataRefs(visitor func(EntryDataRef) error) error {
	if visitor == nil {
		return nil
	}
	r.entryDataActive = false
	offsets, err := r.currentEntryDataOffsets()
	if err != nil {
		return err
	}
	for _, dataOff := range offsets {
		header, err := r.readDataHeaderAt(dataOff)
		if err != nil {
			return err
		}
		payloadOffset := r.dataPayloadOffset()
		if header.object.size < payloadOffset {
			return errCorruptObject
		}
		if err := visitor(EntryDataRef{
			Offset:     dataOff,
			Compressed: dataObjectCompressed(header),
			PayloadLen: header.object.size - payloadOffset,
		}); err != nil {
			return err
		}
	}
	return nil
}

func (dr *DirectoryReader) VisitEntryDataRefs(visitor func(EntryDataRef) error) error {
	r, err := dr.currentReader()
	if err != nil {
		return err
	}
	return r.VisitEntryDataRefs(visitor)
}

// FieldDataOffsets walks the FIELD linkage and returns DATA object offsets for
// every unique value of field.
func (r *Reader) FieldDataOffsets(field []byte) ([]uint64, error) {
	offset, ok, err := r.findFieldHeadDataOffset(field)
	if err != nil || !ok {
		return nil, err
	}
	var offsets []uint64
	for offset != 0 {
		header, err := r.readDataHeaderAt(offset)
		if err != nil {
			return nil, err
		}
		offsets = append(offsets, offset)
		offset = header.nextFieldOffset
	}
	return offsets, nil
}

func (r *Reader) ExplorerQuery(query ExplorerQuery) (ExplorerQueryResult, error) {
	r.clearEntryDataState()
	return r.executeExplorerQuery(query)
}

func (r *Reader) ExplorerUnique(query ExplorerUniqueQuery) (ExplorerUniqueResult, error) {
	r.clearEntryDataState()
	return r.executeExplorerUnique(query)
}

func (dr *DirectoryReader) ExplorerQuery(query ExplorerQuery) (ExplorerQueryResult, error) {
	var combined ExplorerQueryResult
	facetMaps := make(map[string]map[string]explorerFacetBucket)
	var rows []explorerRowWithKey
	for _, reader := range dr.files {
		perFile := query
		perFile.Limit = nil
		reader.clearEntryDataState()
		fileResult, err := reader.executeExplorerQueryWithKeys(perFile)
		if err != nil {
			return ExplorerQueryResult{}, err
		}
		result := fileResult.result
		if len(fileResult.rowKeys) != len(result.Rows) {
			return ExplorerQueryResult{}, fmt.Errorf("%w: explorer directory row-key mismatch", errInvalidJournal)
		}
		mergeExplorerCounters(&combined.Counters, &result.Counters)
		combined.TotalCandidates += result.TotalCandidates
		for i, row := range result.Rows {
			rows = append(rows, explorerRowWithKey{row: row, key: fileResult.rowKeys[i]})
		}
		for _, facet := range result.Facets {
			fieldKey := string(facet.Field)
			values := facetMaps[fieldKey]
			if values == nil {
				values = make(map[string]explorerFacetBucket)
				facetMaps[fieldKey] = values
			}
			for _, value := range facet.Values {
				valueKey := string(value.Value)
				bucket := values[valueKey]
				if bucket.value == nil {
					bucket.value = cloneBytes(value.Value)
				}
				bucket.count += value.Count
				values[valueKey] = bucket
			}
		}
	}
	sortExplorerRows(dr, rows, query.Direction)
	for _, item := range rows {
		combined.Rows = append(combined.Rows, item.row)
	}
	if query.Limit != nil && *query.Limit < len(combined.Rows) {
		combined.Rows = combined.Rows[:*query.Limit]
	}
	combined.Facets = explorerFacetMapsToVec(facetMaps)
	return combined, nil
}

func (dr *DirectoryReader) ExplorerUnique(query ExplorerUniqueQuery) (ExplorerUniqueResult, error) {
	valueCounts := make(map[string]explorerUniqueBucket)
	var counters ExplorerQueryCounters
	var considered uint64
	for _, reader := range dr.files {
		perFile := query
		perFile.Limit = nil
		perFile.Skip = 0
		result, err := reader.ExplorerUnique(perFile)
		if err != nil {
			return ExplorerUniqueResult{}, err
		}
		mergeExplorerCounters(&counters, &result.Counters)
		considered += result.TotalValuesConsidered
		for _, value := range result.Values {
			key := string(value.Value)
			bucket := valueCounts[key]
			if bucket.value == nil {
				bucket.value = cloneBytes(value.Value)
			}
			if value.Count != nil {
				bucket.count += *value.Count
			}
			valueCounts[key] = bucket
		}
	}
	values := make([]ExplorerUniqueValue, 0, len(valueCounts))
	for _, bucket := range valueCounts {
		values = append(values, uniqueValue(bucket.value, bucket.count, query.IncludeCounts))
	}
	sort.Slice(values, func(i, j int) bool {
		return bytes.Compare(values[i].Value, values[j].Value) < 0
	})
	values = paginateUnique(values, query.Skip, query.Limit)
	return ExplorerUniqueResult{
		Field:                 cloneBytes(query.Field),
		Values:                values,
		TotalValuesConsidered: considered,
		Counters:              counters,
	}, nil
}

func (r *Reader) executeExplorerQuery(query ExplorerQuery) (ExplorerQueryResult, error) {
	result, err := r.executeExplorerQueryWithKeys(query)
	if err != nil {
		return ExplorerQueryResult{}, err
	}
	return result.result, nil
}

func (r *Reader) executeExplorerQueryWithKeys(query ExplorerQuery) (explorerQueryResultWithKeys, error) {
	query = normalizeExplorerQuery(query)
	var counters ExplorerQueryCounters
	allOffsets := append([]uint64(nil), r.entryOffsets...)
	counters.EntryOffsetsIndexed = uint64(len(allOffsets))

	candidateSet, err := r.buildCandidateSet(allOffsets, query.Filters, &counters)
	if err != nil {
		return explorerQueryResultWithKeys{}, err
	}
	constrained := constrainedPositiveFacets(query)
	constrainedComplete := query.FullText == nil &&
		len(query.Facets) != 0 &&
		len(constrained) == len(query.Facets)
	if constrainedComplete {
		constrainedComplete, err = r.constrainedFacetsCoverCandidateValues(constrained, candidateSet)
		if err != nil {
			return explorerQueryResultWithKeys{}, err
		}
	}
	noScanPath := query.FullText == nil && (len(query.Facets) == 0 || constrainedComplete)

	facetMaps := make(map[string]map[string]explorerFacetBucket)
	if noScanPath && len(query.Facets) != 0 {
		facetMaps, err = r.constrainedFacetCounts(query, candidateSet, &counters)
		if err != nil {
			return explorerQueryResultWithKeys{}, err
		}
	}
	exactTotal, hasExactTotal := exactCandidateTotal(query, candidateSet, len(allOffsets))

	facetData := fieldDataMap{}
	if !noScanPath && len(query.Facets) != 0 {
		facetData, err = r.buildFieldDataMap(query.Facets, &counters)
		if err != nil {
			return explorerQueryResultWithKeys{}, err
		}
	}
	displayData := fieldDataMap{}
	if query.Display.Mode == ExplorerDisplayFields {
		displayData, err = r.buildFieldDataMap(query.Display.Fields, &counters)
		if err != nil {
			return explorerQueryResultWithKeys{}, err
		}
	}

	var rows []ExplorerRow
	var rowKeys []directoryEntryKey
	totalCandidates := exactTotal
	dataOffsets := make([]uint64, 0, 64)
	ordered := orderedExplorerOffsets(allOffsets, query.Direction)
	needsDataOffsets := query.FullText != nil ||
		(!noScanPath && len(query.Facets) != 0) ||
		query.Display.Mode != ExplorerDisplayNone

	for _, entryOffset := range ordered {
		if noScanPath && hasExactTotal && query.Limit != nil && len(rows) >= *query.Limit {
			break
		}
		if !candidateContains(candidateSet, entryOffset) {
			continue
		}
		entryHdr, offsets, err := r.entryHeaderAndMaybeDataOffsets(entryOffset, needsDataOffsets, dataOffsets)
		if err != nil {
			return explorerQueryResultWithKeys{}, err
		}
		dataOffsets = offsets
		if !timeMatchesExplorer(query.SinceRealtimeUsec, query.UntilRealtimeUsec, entryHdr.realtime) {
			continue
		}
		if !hasExactTotal {
			totalCandidates++
		}
		counters.CandidateEntries++

		if query.FullText != nil {
			matched, err := r.entryMatchesFullText(dataOffsets, query.FullText, &counters)
			if err != nil {
				return explorerQueryResultWithKeys{}, err
			}
			if !matched {
				continue
			}
		}

		if !noScanPath && len(query.Facets) != 0 {
			counters.CandidateDataRefsVisited += uint64(len(dataOffsets))
			if err := r.aggregateFacets(dataOffsets, facetData, facetMaps, &counters); err != nil {
				return explorerQueryResultWithKeys{}, err
			}
		}

		if query.Limit == nil || len(rows) < *query.Limit {
			fields, err := r.materializeDisplayFields(dataOffsets, query.Display, displayData, &counters)
			if err != nil {
				return explorerQueryResultWithKeys{}, err
			}
			if query.Display.Mode != ExplorerDisplayNone {
				counters.DisplayRowsExpanded++
			}
			key := directoryEntryKey{
				seqnumID:  r.header.seqnumID,
				seqnum:    entryHdr.seqnum,
				bootID:    entryHdr.bootID,
				monotonic: entryHdr.monotonic,
				realtime:  entryHdr.realtime,
				xorHash:   entryHdr.xorHash,
			}
			rows = append(rows, ExplorerRow{
				Realtime: entryHdr.realtime,
				Seqnum:   entryHdr.seqnum,
				Cursor:   r.makeCursor(entryOffset, entryHdr),
				Fields:   fields,
			})
			rowKeys = append(rowKeys, key)
		}
	}

	return explorerQueryResultWithKeys{
		result: ExplorerQueryResult{
			Rows:            rows,
			Facets:          explorerFacetMapsToVec(facetMaps),
			TotalCandidates: totalCandidates,
			Counters:        counters,
		},
		rowKeys: rowKeys,
	}, nil
}

func (r *Reader) executeExplorerUnique(query ExplorerUniqueQuery) (ExplorerUniqueResult, error) {
	var counters ExplorerQueryCounters
	allOffsets := append([]uint64(nil), r.entryOffsets...)
	counters.EntryOffsetsIndexed = uint64(len(allOffsets))
	candidateSet, err := r.buildCandidateSet(allOffsets, query.Filters, &counters)
	if err != nil {
		return ExplorerUniqueResult{}, err
	}
	targetOffsets, err := r.FieldDataOffsets(query.Field)
	if err != nil {
		return ExplorerUniqueResult{}, err
	}
	counters.FieldLinkageHits += uint64(len(targetOffsets))

	values := make([]ExplorerUniqueValue, 0, len(targetOffsets))
	postings := make([]uint64, 0, 64)
	var considered uint64
	for _, dataOffset := range targetOffsets {
		considered++
		postings, err = r.collectDataEntryOffsets(dataOffset, postings)
		if err != nil {
			return ExplorerUniqueResult{}, err
		}
		var count uint64
		for _, entryOffset := range postings {
			if !candidateContains(candidateSet, entryOffset) {
				continue
			}
			entryHdr, err := r.readEntryHeaderAt(entryOffset)
			if err != nil {
				return ExplorerUniqueResult{}, err
			}
			if timeMatchesExplorer(query.SinceRealtimeUsec, query.UntilRealtimeUsec, entryHdr.realtime) {
				count++
			}
		}
		if count == 0 {
			continue
		}
		value, err := r.materializeKnownFieldValue(dataOffset, query.Field, &counters)
		if err != nil {
			return ExplorerUniqueResult{}, err
		}
		values = append(values, uniqueValue(value, count, query.IncludeCounts))
	}

	sort.Slice(values, func(i, j int) bool {
		return bytes.Compare(values[i].Value, values[j].Value) < 0
	})
	values = paginateUnique(values, query.Skip, query.Limit)
	return ExplorerUniqueResult{
		Field:                 cloneBytes(query.Field),
		Values:                values,
		TotalValuesConsidered: considered,
		Counters:              counters,
	}, nil
}

func normalizeExplorerQuery(query ExplorerQuery) ExplorerQuery {
	if query.Limit != nil && *query.Limit < 0 {
		zero := 0
		query.Limit = &zero
	}
	return query
}

func (r *Reader) buildCandidateSet(allOffsets []uint64, filters []ExplorerFilter, counters *ExplorerQueryCounters) (map[uint64]struct{}, error) {
	var candidate map[uint64]struct{}
	for _, filter := range filters {
		valueOffsets, err := r.filterValueEntryOffsets(filter, counters)
		if err != nil {
			return nil, err
		}
		switch filter.Kind {
		case ExplorerFilterIn:
			if candidate == nil {
				candidate = valueOffsets
			} else {
				candidate = intersectOffsetSets(candidate, valueOffsets)
			}
		case ExplorerFilterNotIn:
			if candidate == nil {
				candidate = make(map[uint64]struct{}, len(allOffsets))
				for _, offset := range allOffsets {
					candidate[offset] = struct{}{}
				}
			}
			for offset := range valueOffsets {
				delete(candidate, offset)
			}
		default:
			return nil, fmt.Errorf("%w: unsupported explorer filter kind", errInvalidJournal)
		}
	}
	return candidate, nil
}

func (r *Reader) filterValueEntryOffsets(filter ExplorerFilter, counters *ExplorerQueryCounters) (map[uint64]struct{}, error) {
	out := make(map[uint64]struct{})
	postings := make([]uint64, 0, 64)
	for _, value := range filter.Values {
		payload := payloadFor(filter.Field, value)
		hash := r.hash(payload)
		dataOffset, ok, err := r.findDataOffset(hash, payload)
		if err != nil {
			return nil, err
		}
		if !ok {
			continue
		}
		counters.FilterDataObjectsExamined++
		postings, err = r.collectDataEntryOffsets(dataOffset, postings)
		if err != nil {
			return nil, err
		}
		for _, entryOffset := range postings {
			out[entryOffset] = struct{}{}
		}
	}
	return out, nil
}

func (r *Reader) findDataOffset(hash uint64, payload []byte) (uint64, bool, error) {
	if r.header.dataHashTableOffset == 0 || r.header.dataHashTableSize < hashItemSize {
		return 0, false, nil
	}
	buckets := r.header.dataHashTableSize / hashItemSize
	if buckets == 0 {
		return 0, false, nil
	}
	bucketOffset := r.header.dataHashTableOffset + (hash%buckets)*hashItemSize
	itemBuf, err := r.readSlice(bucketOffset, hashItemSize)
	if err != nil {
		return 0, false, err
	}
	item := parseHashItem(itemBuf)
	for offset := item.head; offset != 0; {
		header, err := r.readDataHeaderAt(offset)
		if err != nil {
			return 0, false, err
		}
		if header.hash == hash {
			stored, err := r.readDataPayload(offset)
			if err != nil {
				return 0, false, err
			}
			if bytes.Equal(stored, payload) {
				return offset, true, nil
			}
		}
		offset = header.nextHashOffset
	}
	return 0, false, nil
}

func (r *Reader) collectDataEntryOffsets(dataOffset uint64, dst []uint64) ([]uint64, error) {
	dst = dst[:0]
	header, err := r.readDataHeaderAt(dataOffset)
	if err != nil {
		return dst, err
	}
	if header.nEntries == 0 {
		return dst, nil
	}
	if header.entryOffset != 0 {
		dst = append(dst, header.entryOffset)
	}
	remaining := header.nEntries - 1
	offset := header.entryArrayOffset
	for offset != 0 && remaining > 0 {
		arrayHeader, capacity, err := r.readOffsetArrayHeader(offset)
		if err != nil {
			return dst, err
		}
		toRead := capacity
		if remaining < toRead {
			toRead = remaining
		}
		itemSize := r.offsetArrayItemSize()
		itemsOffset := offset + offsetArrayObjectHeaderSize
		itemsSize := toRead * itemSize
		items, err := r.readSlice(itemsOffset, itemsSize)
		if err != nil {
			return dst, err
		}
		if itemSize == compactOffsetArrayItemSize {
			for pos := 0; pos < len(items); pos += compactOffsetArrayItemSize {
				entryOffset := uint64(binaryLittleEndianUint32(items[pos : pos+compactOffsetArrayItemSize]))
				if entryOffset != 0 {
					dst = append(dst, entryOffset)
				}
			}
		} else {
			for pos := 0; pos < len(items); pos += regularOffsetArrayItemSize {
				entryOffset := binaryLittleEndianUint64(items[pos : pos+regularOffsetArrayItemSize])
				if entryOffset != 0 {
					dst = append(dst, entryOffset)
				}
			}
		}
		remaining -= toRead
		offset = arrayHeader.nextArrayOffset
	}
	return dst, nil
}

func (r *Reader) entryHeaderAndMaybeDataOffsets(entryOffset uint64, needsDataOffsets bool, dst []uint64) (entryHeader, []uint64, error) {
	if !needsDataOffsets {
		header, err := r.readEntryHeaderAt(entryOffset)
		return header, dst[:0], err
	}
	header, offsets, err := r.readEntryDataOffsetsAt(entryOffset, dst)
	return header, offsets, err
}

func (r *Reader) entryMatchesFullText(dataOffsets []uint64, needle []byte, counters *ExplorerQueryCounters) (bool, error) {
	if len(needle) == 0 {
		return true, nil
	}
	counters.CandidateDataRefsVisited += uint64(len(dataOffsets))
	for _, dataOffset := range dataOffsets {
		payload, err := r.materializePayload(dataOffset, counters)
		if err != nil {
			return false, err
		}
		counters.FTSPayloadsScanned++
		if containsExplorerBytes(payload, needle) {
			return true, nil
		}
	}
	return false, nil
}

func (r *Reader) buildFieldDataMap(fields [][]byte, counters *ExplorerQueryCounters) (fieldDataMap, error) {
	out := fieldDataMap{offsets: make(map[uint64]int)}
	var unique [][]byte
	for _, field := range fields {
		found := false
		for _, existing := range unique {
			if bytes.Equal(existing, field) {
				found = true
				break
			}
		}
		if !found {
			unique = append(unique, field)
		}
	}
	for _, field := range unique {
		index := len(out.fields)
		offsets, err := r.FieldDataOffsets(field)
		if err != nil {
			return fieldDataMap{}, err
		}
		counters.FieldLinkageHits += uint64(len(offsets))
		for _, offset := range offsets {
			out.offsets[offset] = index
		}
		out.fields = append(out.fields, cloneBytes(field))
	}
	return out, nil
}

func (r *Reader) aggregateFacets(dataOffsets []uint64, data fieldDataMap, facetMaps map[string]map[string]explorerFacetBucket, counters *ExplorerQueryCounters) error {
	for _, dataOffset := range dataOffsets {
		fieldIndex, ok := data.offsets[dataOffset]
		if !ok {
			continue
		}
		field := data.fields[fieldIndex]
		value, err := r.materializeKnownFieldValue(dataOffset, field, counters)
		if err != nil {
			return err
		}
		counters.FacetValuesMaterialized++
		fieldKey := string(field)
		values := facetMaps[fieldKey]
		if values == nil {
			values = make(map[string]explorerFacetBucket)
			facetMaps[fieldKey] = values
		}
		valueKey := string(value)
		bucket := values[valueKey]
		if bucket.value == nil {
			bucket.value = value
		}
		bucket.count++
		values[valueKey] = bucket
	}
	return nil
}

func (r *Reader) materializeDisplayFields(dataOffsets []uint64, display ExplorerDisplay, displayData fieldDataMap, counters *ExplorerQueryCounters) ([]RawField, error) {
	switch display.Mode {
	case ExplorerDisplayNone:
		return nil, nil
	case ExplorerDisplayAll:
		fields := make([]RawField, 0, len(dataOffsets))
		for _, dataOffset := range dataOffsets {
			payload, err := r.materializePayload(dataOffset, counters)
			if err != nil {
				return nil, err
			}
			name, value, ok := splitRawPayload(payload)
			if !ok {
				counters.FieldLinkageFallbacks++
				continue
			}
			fields = append(fields, RawField{Name: cloneBytes(name), Value: cloneBytes(value)})
		}
		return fields, nil
	case ExplorerDisplayFields:
		fields := make([]RawField, 0, len(displayData.fields))
		for _, dataOffset := range dataOffsets {
			fieldIndex, ok := displayData.offsets[dataOffset]
			if !ok {
				continue
			}
			field := displayData.fields[fieldIndex]
			value, err := r.materializeKnownFieldValue(dataOffset, field, counters)
			if err != nil {
				return nil, err
			}
			fields = append(fields, RawField{Name: cloneBytes(field), Value: value})
		}
		return fields, nil
	default:
		return nil, fmt.Errorf("%w: unsupported explorer display mode", errInvalidJournal)
	}
}

func (r *Reader) materializeKnownFieldValue(dataOffset uint64, field []byte, counters *ExplorerQueryCounters) ([]byte, error) {
	payload, err := r.materializePayload(dataOffset, counters)
	if err != nil {
		return nil, err
	}
	if len(payload) > len(field) && bytes.Equal(payload[:len(field)], field) && payload[len(field)] == '=' {
		return cloneBytes(payload[len(field)+1:]), nil
	}

	counters.FieldLinkageFallbacks++
	name, value, ok := splitRawPayload(payload)
	if !ok {
		return nil, fmt.Errorf("%w: DATA payload has no FIELD=value separator", errCorruptObject)
	}
	if !bytes.Equal(name, field) {
		return nil, fmt.Errorf("%w: FIELD linkage DATA payload mismatch", errCorruptObject)
	}
	return cloneBytes(value), nil
}

func (r *Reader) materializePayload(dataOffset uint64, counters *ExplorerQueryCounters) ([]byte, error) {
	header, err := r.readDataHeaderAt(dataOffset)
	if err != nil {
		return nil, err
	}
	counters.PayloadsMaterialized++
	// The returned slice is ephemeral: it may alias mmap/read buffers or
	// reader-owned decompression scratch. Callers must consume it immediately or
	// clone it before the next materializePayload call.
	payloadOffset := r.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return nil, errCorruptObject
	}
	payloadLen := header.object.size - payloadOffset
	payload, err := r.readSlice(dataOffset+payloadOffset, payloadLen)
	if err != nil {
		return nil, err
	}
	if !dataObjectCompressed(header) {
		return payload, nil
	}

	counters.PayloadsDecompressed++
	if header.object.flag&objectCompressedZSTD != 0 {
		if r.explorerZstdDecoder == nil {
			decoder, err := zstd.NewReader(nil, zstd.WithDecoderMaxMemory(uint64(maxUncompressedDataObjectSize)))
			if err != nil {
				return nil, err
			}
			r.explorerZstdDecoder = decoder
		}
		decoded, err := r.explorerZstdDecoder.DecodeAll(payload, r.explorerDecompressScratch[:0])
		if err != nil {
			return nil, err
		}
		r.explorerDecompressScratch = decoded
		return decoded, nil
	}
	if header.object.flag&objectCompressedXZ != 0 {
		reader, err := xz.NewReader(bytes.NewReader(payload))
		if err != nil {
			return nil, err
		}
		return readAllLimited(reader, maxUncompressedDataObjectSize)
	}
	if header.object.flag&objectCompressedLZ4 != 0 {
		if len(payload) < 8 {
			return nil, errors.New("lz4 compressed payload too short")
		}
		uncompressedSize := binary.LittleEndian.Uint64(payload[:8])
		if uncompressedSize > maxUncompressedDataObjectSize {
			return nil, errors.New("lz4 decompressed payload too large")
		}
		if uint64(int(uncompressedSize)) != uncompressedSize {
			return nil, errors.New("lz4 decompressed payload too large for platform")
		}
		compressedData := payload[8:]
		if cap(r.explorerDecompressScratch) < int(uncompressedSize) {
			r.explorerDecompressScratch = make([]byte, int(uncompressedSize))
		}
		decoded := r.explorerDecompressScratch[:int(uncompressedSize)]
		n, err := lz4.UncompressBlock(compressedData, decoded)
		if err != nil {
			return nil, err
		}
		if uint64(n) != uncompressedSize {
			return nil, errors.New("lz4 decompressed size mismatch")
		}
		return decoded, nil
	}
	return payload, nil
}

func constrainedPositiveFacets(query ExplorerQuery) map[string][][]byte {
	positive := make(map[string][][]byte)
	for _, filter := range query.Filters {
		if filter.Kind == ExplorerFilterIn && len(filter.Values) != 0 {
			positive[string(filter.Field)] = filter.Values
		}
	}
	constrained := make(map[string][][]byte)
	for _, facet := range query.Facets {
		if values, ok := positive[string(facet)]; ok {
			constrained[string(facet)] = values
		}
	}
	return constrained
}

func (r *Reader) constrainedFacetCounts(query ExplorerQuery, candidateSet map[uint64]struct{}, counters *ExplorerQueryCounters) (map[string]map[string]explorerFacetBucket, error) {
	maps := make(map[string]map[string]explorerFacetBucket)
	constrained := constrainedPositiveFacets(query)
	for fieldKey, values := range constrained {
		field := []byte(fieldKey)
		for _, value := range values {
			offsets, err := r.filterValueEntryOffsets(FieldIn(field, value), counters)
			if err != nil {
				return nil, err
			}
			var count uint64
			for entryOffset := range offsets {
				if !candidateContains(candidateSet, entryOffset) {
					continue
				}
				entryHdr, err := r.readEntryHeaderAt(entryOffset)
				if err != nil {
					return nil, err
				}
				if timeMatchesExplorer(query.SinceRealtimeUsec, query.UntilRealtimeUsec, entryHdr.realtime) {
					count++
				}
			}
			counters.ConstrainedFacetCounts++
			valuesMap := maps[fieldKey]
			if valuesMap == nil {
				valuesMap = make(map[string]explorerFacetBucket)
				maps[fieldKey] = valuesMap
			}
			valuesMap[string(value)] = explorerFacetBucket{value: cloneBytes(value), count: count}
		}
	}
	return maps, nil
}

func (r *Reader) constrainedFacetsCoverCandidateValues(constrained map[string][][]byte, candidateSet map[uint64]struct{}) (bool, error) {
	postings := make([]uint64, 0, 64)
	for fieldKey, values := range constrained {
		field := []byte(fieldKey)
		selected := make(map[uint64]struct{}, len(values))
		for _, value := range values {
			payload := payloadFor(field, value)
			offset, ok, err := r.findDataOffset(r.hash(payload), payload)
			if err != nil {
				return false, err
			}
			if ok {
				selected[offset] = struct{}{}
			}
		}
		fieldOffsets, err := r.FieldDataOffsets(field)
		if err != nil {
			return false, err
		}
		for _, dataOffset := range fieldOffsets {
			if _, ok := selected[dataOffset]; ok {
				continue
			}
			postings, err = r.collectDataEntryOffsets(dataOffset, postings)
			if err != nil {
				return false, err
			}
			if candidateSet == nil {
				if len(postings) != 0 {
					return false, nil
				}
				continue
			}
			for _, entryOffset := range postings {
				if _, ok := candidateSet[entryOffset]; ok {
					return false, nil
				}
			}
		}
	}
	return true, nil
}

func exactCandidateTotal(query ExplorerQuery, candidateSet map[uint64]struct{}, allLen int) (uint64, bool) {
	if query.FullText != nil || query.SinceRealtimeUsec != nil || query.UntilRealtimeUsec != nil {
		return 0, false
	}
	if candidateSet == nil {
		return uint64(allLen), true
	}
	return uint64(len(candidateSet)), true
}

func intersectOffsetSets(a, b map[uint64]struct{}) map[uint64]struct{} {
	if len(a) > len(b) {
		a, b = b, a
	}
	out := make(map[uint64]struct{}, len(a))
	for offset := range a {
		if _, ok := b[offset]; ok {
			out[offset] = struct{}{}
		}
	}
	return out
}

func candidateContains(candidate map[uint64]struct{}, offset uint64) bool {
	if candidate == nil {
		return true
	}
	_, ok := candidate[offset]
	return ok
}

func orderedExplorerOffsets(offsets []uint64, direction Direction) []uint64 {
	out := append([]uint64(nil), offsets...)
	if direction == DirectionBackward {
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	}
	return out
}

func timeMatchesExplorer(since, until *uint64, realtime uint64) bool {
	if since != nil && realtime < *since {
		return false
	}
	if until != nil && realtime >= *until {
		return false
	}
	return true
}

func payloadFor(field, value []byte) []byte {
	payload := make([]byte, 0, len(field)+1+len(value))
	payload = append(payload, field...)
	payload = append(payload, '=')
	payload = append(payload, value...)
	return payload
}

func containsExplorerBytes(haystack, needle []byte) bool {
	if len(needle) == 0 {
		return true
	}
	return bytes.Contains(haystack, needle)
}

func dataObjectCompressed(header dataHeader) bool {
	return header.object.flag&(objectCompressedZSTD|objectCompressedXZ|objectCompressedLZ4) != 0
}

type explorerFacetBucket struct {
	value []byte
	count uint64
}

func explorerFacetMapsToVec(facetMaps map[string]map[string]explorerFacetBucket) []ExplorerFacet {
	facets := make([]ExplorerFacet, 0, len(facetMaps))
	for fieldKey, valuesMap := range facetMaps {
		values := make([]ExplorerFacetValue, 0, len(valuesMap))
		for _, bucket := range valuesMap {
			values = append(values, ExplorerFacetValue{Value: bucket.value, Count: bucket.count})
		}
		sort.Slice(values, func(i, j int) bool {
			return bytes.Compare(values[i].Value, values[j].Value) < 0
		})
		facets = append(facets, ExplorerFacet{Field: []byte(fieldKey), Values: values})
	}
	sort.Slice(facets, func(i, j int) bool {
		return bytes.Compare(facets[i].Field, facets[j].Field) < 0
	})
	return facets
}

func sortExplorerRows(dr *DirectoryReader, rows []explorerRowWithKey, direction Direction) {
	sort.Slice(rows, func(i, j int) bool {
		if direction == DirectionBackward {
			return dr.compareEntryKeys(rows[j].key, rows[i].key) < 0
		}
		return dr.compareEntryKeys(rows[i].key, rows[j].key) < 0
	})
}

type explorerUniqueBucket struct {
	value []byte
	count uint64
}

func uniqueValue(value []byte, count uint64, includeCounts bool) ExplorerUniqueValue {
	var countPtr *uint64
	if includeCounts {
		c := count
		countPtr = &c
	}
	return ExplorerUniqueValue{Value: cloneBytes(value), Count: countPtr}
}

func paginateUnique(values []ExplorerUniqueValue, skip int, limit *int) []ExplorerUniqueValue {
	if skip < 0 {
		skip = 0
	}
	if skip > len(values) {
		return nil
	}
	end := len(values)
	if limit != nil && skip+*limit < end {
		end = skip + *limit
	}
	return append([]ExplorerUniqueValue(nil), values[skip:end]...)
}

func mergeExplorerCounters(dst, src *ExplorerQueryCounters) {
	dst.EntryOffsetsIndexed += src.EntryOffsetsIndexed
	dst.FilterDataObjectsExamined += src.FilterDataObjectsExamined
	dst.CandidateEntries += src.CandidateEntries
	dst.CandidateDataRefsVisited += src.CandidateDataRefsVisited
	dst.PayloadsMaterialized += src.PayloadsMaterialized
	dst.PayloadsDecompressed += src.PayloadsDecompressed
	dst.FacetValuesMaterialized += src.FacetValuesMaterialized
	dst.FTSPayloadsScanned += src.FTSPayloadsScanned
	dst.DisplayRowsExpanded += src.DisplayRowsExpanded
	dst.ConstrainedFacetCounts += src.ConstrainedFacetCounts
	dst.FieldLinkageHits += src.FieldLinkageHits
	dst.FieldLinkageFallbacks += src.FieldLinkageFallbacks
}

func binaryLittleEndianUint32(src []byte) uint32 {
	return uint32(src[0]) | uint32(src[1])<<8 | uint32(src[2])<<16 | uint32(src[3])<<24
}

func binaryLittleEndianUint64(src []byte) uint64 {
	return uint64(src[0]) |
		uint64(src[1])<<8 |
		uint64(src[2])<<16 |
		uint64(src[3])<<24 |
		uint64(src[4])<<32 |
		uint64(src[5])<<40 |
		uint64(src[6])<<48 |
		uint64(src[7])<<56
}
