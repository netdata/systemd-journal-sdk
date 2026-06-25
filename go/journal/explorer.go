package journal

import (
	"bytes"
	"errors"
	"fmt"
	"math/bits"
	"sort"
	"strconv"
	"time"
)

const (
	defaultHistogramTargetBuckets = 150
	defaultTimeSlackUsec          = 120_000_000
	explorerControlCheckEveryRows = 8192
	defaultRowsFullCheckEveryRows = 1
	explorerSamplingSlotsMax      = 1000
	explorerSamplingRecalibrate   = 10_000
	explorerSamplingEstimateAfter = 0.01

	facetPublic         uint8 = 0x01
	facetHistogram      uint8 = 0x02
	facetSourceRealtime uint8 = 0x04
)

var (
	sourceRealtimeField      = []byte("_SOURCE_REALTIME_TIMESTAMP")
	explorerUnsetValue       = []byte("-")
	explorerUnsampledValue   = []byte("[unsampled]")
	explorerEstimatedValue   = []byte("[estimated]")
	errExplorerVerification  = errors.New("explorer verification failed")
	errExplorerDebugDisabled = errors.New("debug_collect_column_fields_by_row_traversal is a debug-only discrepancy tool; production explorer queries must use FIELD-index column catalogs instead")
)

type ExplorerAnchorKind int

const (
	ExplorerAnchorAuto ExplorerAnchorKind = iota
	ExplorerAnchorHead
	ExplorerAnchorTail
	ExplorerAnchorRealtime
)

type ExplorerAnchor struct {
	Kind         ExplorerAnchorKind
	RealtimeUsec uint64
}

func DefaultExplorerAnchor() ExplorerAnchor {
	return ExplorerAnchor{Kind: ExplorerAnchorAuto}
}

func RealtimeExplorerAnchor(usec uint64) ExplorerAnchor {
	return ExplorerAnchor{Kind: ExplorerAnchorRealtime, RealtimeUsec: usec}
}

type ExplorerFieldMode int

const (
	ExplorerFieldModeAllValues ExplorerFieldMode = iota
	ExplorerFieldModeFirstValue
)

type ExplorerStrategy int

const (
	ExplorerStrategyTraversal ExplorerStrategy = iota
	ExplorerStrategyIndex
	ExplorerStrategyCompare
)

type ExplorerFilter struct {
	Field  []byte
	Values [][]byte
}

func NewExplorerFilter(field []byte, values ...[]byte) ExplorerFilter {
	out := ExplorerFilter{Field: cloneBytes(field), Values: make([][]byte, 0, len(values))}
	for _, value := range values {
		out.Values = append(out.Values, cloneBytes(value))
	}
	return out
}

type ExplorerQuery struct {
	AfterRealtimeUsec   *uint64
	BeforeRealtimeUsec  *uint64
	Anchor              ExplorerAnchor
	Direction           Direction
	Limit               int
	Filters             []ExplorerFilter
	Facets              [][]byte
	Histogram           []byte
	HistogramAfterUsec  *uint64
	HistogramBeforeUsec *uint64
	HistogramBuckets    int
	FTSTerms            []ExplorerFtsPattern
	FTSPatterns         [][]byte
	FTSNegative         [][]byte
	FieldMode           ExplorerFieldMode

	ExcludeFacetFieldFilters bool
	UseSourceRealtime        bool
	RealtimeSlackUsec        uint64
	StopWhenRowsFull         bool
	StopWhenRowsFullEvery    uint64
	Sampling                 *ExplorerSampling

	DebugCollectColumnFieldsByRowTraversal bool
}

func DefaultExplorerQuery() ExplorerQuery {
	return ExplorerQuery{
		Anchor:                   DefaultExplorerAnchor(),
		Direction:                DirectionForward,
		Limit:                    200,
		HistogramBuckets:         defaultHistogramTargetBuckets,
		FieldMode:                ExplorerFieldModeFirstValue,
		ExcludeFacetFieldFilters: true,
		UseSourceRealtime:        true,
		RealtimeSlackUsec:        defaultTimeSlackUsec,
		StopWhenRowsFullEvery:    defaultRowsFullCheckEveryRows,
	}
}

func (q ExplorerQuery) WithFilter(field []byte, values ...[]byte) ExplorerQuery {
	q.Filters = append(q.Filters, NewExplorerFilter(field, values...))
	return q
}

func (q ExplorerQuery) WithFacet(field []byte) ExplorerQuery {
	q.Facets = append(q.Facets, cloneBytes(field))
	return q
}

func (q ExplorerQuery) WithHistogram(field []byte) ExplorerQuery {
	q.Histogram = cloneBytes(field)
	return q
}

func (q ExplorerQuery) WithFTSPattern(pattern []byte) ExplorerQuery {
	q.FTSTerms = append(q.FTSTerms, NewExplorerFtsPattern(pattern, false))
	q.FTSPatterns = append(q.FTSPatterns, cloneBytes(pattern))
	return q
}

func (q ExplorerQuery) WithFTSNegativePattern(pattern []byte) ExplorerQuery {
	q.FTSTerms = append(q.FTSTerms, NewExplorerFtsPattern(pattern, true))
	q.FTSNegative = append(q.FTSNegative, cloneBytes(pattern))
	return q
}

type ExplorerSampling struct {
	Budget               uint64
	MatchedFiles         uint64
	FileHeadRealtimeUsec uint64
	FileTailRealtimeUsec uint64
	FileHeadSeqnum       uint64
	FileTailSeqnum       uint64
	FileEntries          uint64
}

type ExplorerFtsPattern struct {
	Parts    [][]byte
	Negative bool
}

func NewExplorerFtsPattern(pattern []byte, negative bool) ExplorerFtsPattern {
	parts := bytes.Split(pattern, []byte{'*'})
	out := ExplorerFtsPattern{Negative: negative}
	for _, part := range parts {
		if len(part) != 0 {
			out.Parts = append(out.Parts, cloneBytes(part))
		}
	}
	return out
}

func (p ExplorerFtsPattern) matches(value []byte) bool {
	if len(value) == 0 {
		return false
	}
	if len(p.Parts) == 0 {
		return true
	}
	haystack := value
	for _, part := range p.Parts {
		index := findASCIIInsensitive(haystack, part)
		if index < 0 {
			return false
		}
		haystack = haystack[index+len(part):]
	}
	return true
}

type ExplorerStats struct {
	RowsExamined               uint64 `json:"rows_examined"`
	RowsMatched                uint64 `json:"rows_matched"`
	FacetRowsMatched           uint64 `json:"facet_rows_matched"`
	RowsReturned               uint64 `json:"rows_returned"`
	RowsUnsampled              uint64 `json:"rows_unsampled"`
	RowsEstimated              uint64 `json:"rows_estimated"`
	SamplingSampled            uint64 `json:"sampling_sampled"`
	SamplingUnsampled          uint64 `json:"sampling_unsampled"`
	SamplingEstimated          uint64 `json:"sampling_estimated"`
	LastRealtimeUsec           uint64 `json:"last_realtime_usec"`
	MaxSourceRealtimeDeltaUsec uint64 `json:"max_source_realtime_delta_usec"`
	DataRefsSeen               uint64 `json:"data_refs_seen"`
	DataRefsSkipped            uint64 `json:"data_refs_skipped"`
	DataPayloadsLoaded         uint64 `json:"data_payloads_loaded"`
	DataObjectsClassified      uint64 `json:"data_objects_classified"`
	DataCacheHits              uint64 `json:"data_cache_hits"`
	DataCacheMisses            uint64 `json:"data_cache_misses"`
	PayloadsDecompressed       uint64 `json:"payloads_decompressed"`
	FTSScans                   uint64 `json:"fts_scans"`
	FacetUpdates               uint64 `json:"facet_updates"`
	HistogramUpdates           uint64 `json:"histogram_updates"`
	ReturnedRowExpansions      uint64 `json:"returned_row_expansions"`
	EarlyStopOpportunities     uint64 `json:"early_stop_opportunities"`
	EarlyStops                 uint64 `json:"early_stops"`
}

type ExplorerRow struct {
	RealtimeUsec uint64
	Cursor       string
	Payloads     [][]byte
}

type explorerRowPayloadMode int

const (
	// explorerRowPayloadExpand returns full payloads for visible rows.
	explorerRowPayloadExpand explorerRowPayloadMode = iota
	// explorerRowPayloadCursorOnly returns cursors without row payload expansion.
	explorerRowPayloadCursorOnly
)

type ExplorerHistogramBucket struct {
	StartRealtimeUsec uint64
	EndRealtimeUsec   uint64
	Values            map[string]uint64
}

type ExplorerHistogram struct {
	Field   []byte
	Buckets []ExplorerHistogramBucket
}

type ExplorerComparison struct {
	TraversalDuration time.Duration
	IndexDuration     time.Duration
	TraversalStats    ExplorerStats
	IndexStats        ExplorerStats
}

type ExplorerResult struct {
	Rows         []ExplorerRow
	Facets       map[string]map[string]uint64
	Histogram    *ExplorerHistogram
	ColumnFields map[string]struct{}
	Stats        ExplorerStats
	Comparison   *ExplorerComparison
}

type ExplorerStopReason int

const (
	ExplorerStopNone ExplorerStopReason = iota
	ExplorerStopTimedOut
	ExplorerStopCancelled
)

type ExplorerProgress struct {
	Stats   ExplorerStats
	Elapsed time.Duration
}

type ExplorerControl struct {
	deadline      *time.Time
	cancellation  func() bool
	progress      func(ExplorerProgress)
	candidateRow  func(uint64) bool
	matchedRow    func(uint64, uint64) bool
	sampling      *explorerSamplingState
	progressEvery time.Duration
	started       time.Time
	lastProgress  time.Time
	nextCheckRows uint64
	stopReason    ExplorerStopReason
}

func NewExplorerControl() *ExplorerControl {
	now := time.Now()
	return &ExplorerControl{
		progressEvery: defaultExplorerProgressInterval(),
		started:       now,
		lastProgress:  now,
		nextCheckRows: explorerControlCheckEveryRows,
	}
}

func defaultExplorerProgressInterval() time.Duration {
	return 250 * time.Millisecond
}

func (c *ExplorerControl) SetDeadline(deadline *time.Time) {
	c.deadline = deadline
}

func (c *ExplorerControl) SetCancellationCallback(callback func() bool) {
	c.cancellation = callback
}

func (c *ExplorerControl) SetProgressCallback(callback func(ExplorerProgress)) {
	c.progress = callback
}

func (c *ExplorerControl) setCandidateRowCallback(callback func(uint64) bool) {
	c.candidateRow = callback
}

func (c *ExplorerControl) SetMatchedRowCallback(callback func(uint64, uint64) bool) {
	c.matchedRow = callback
}

func (c *ExplorerControl) setSamplingState(sampling *explorerSamplingState) {
	c.sampling = sampling
}

func (c *ExplorerControl) SetProgressInterval(interval time.Duration) {
	c.progressEvery = interval
}

func (c *ExplorerControl) StopReason() ExplorerStopReason {
	if c == nil {
		return ExplorerStopNone
	}
	return c.stopReason
}

func (c *ExplorerControl) shouldStopAfterRows(rowsSeen uint64, stats ExplorerStats) bool {
	if c == nil {
		return false
	}
	if c.stopReason != ExplorerStopNone {
		return true
	}
	if rowsSeen < c.nextCheckRows {
		return false
	}
	c.nextCheckRows = rowsSeen + explorerControlCheckEveryRows
	return c.check(stats)
}

func (c *ExplorerControl) check(stats ExplorerStats) bool {
	if c == nil {
		return false
	}
	now := time.Now()
	if now.Sub(c.lastProgress) >= c.progressEvery {
		c.emitProgress(stats, now)
	}
	if c.cancellation != nil && c.cancellation() {
		c.stopReason = ExplorerStopCancelled
		c.emitProgress(stats, now)
		return true
	}
	if c.deadline != nil && !now.Before(*c.deadline) {
		c.stopReason = ExplorerStopTimedOut
		c.emitProgress(stats, now)
		return true
	}
	return false
}

func (c *ExplorerControl) emitProgress(stats ExplorerStats, now time.Time) {
	if c == nil {
		return
	}
	c.lastProgress = now
	if c.progress != nil {
		c.progress(ExplorerProgress{Stats: stats, Elapsed: now.Sub(c.started)})
	}
}

func (c *ExplorerControl) emitMatchedRow(realtimeUsec, rowsMatched uint64) bool {
	return c != nil && c.matchedRow != nil && c.matchedRow(realtimeUsec, rowsMatched)
}

type explorerSamplingDecisionKind int

const (
	explorerSamplingFull explorerSamplingDecisionKind = iota
	explorerSamplingSkipFields
	explorerSamplingStopAndEstimate
)

type explorerSamplingDecision struct {
	kind             explorerSamplingDecisionKind
	sampled          bool
	remainingRows    uint64
	fromRealtimeUsec uint64
	toRealtimeUsec   uint64
}

type explorerSamplingState struct {
	startRealtimeUsec    uint64
	endRealtimeUsec      uint64
	fileHeadRealtimeUsec uint64
	fileTailRealtimeUsec uint64
	fileHeadSeqnum       uint64
	fileTailSeqnum       uint64
	fileEntries          uint64
	firstRealtimeUsec    *uint64
	stepRealtimeUsec     uint64
	enableAfterSamples   uint64
	perFileEnableAfter   uint64
	perSlotEnableAfter   uint64
	sampled              uint64
	perFileSampled       uint64
	perFileUnsampled     uint64
	perFileEvery         uint64
	perFileSkipped       uint64
	perFileRecalibrate   uint64
	perSlotSampled       []uint64
	perSlotUnsampled     []uint64
	matchedFiles         uint64
	direction            Direction
}

func newExplorerSamplingState(query ExplorerQuery, histogramBucketCount int) *explorerSamplingState {
	if query.Sampling == nil || query.AfterRealtimeUsec == nil || query.BeforeRealtimeUsec == nil {
		return nil
	}
	sampling := *query.Sampling
	if sampling.Budget == 0 || sampling.MatchedFiles == 0 || *query.AfterRealtimeUsec >= *query.BeforeRealtimeUsec {
		return nil
	}
	slots := explorerSamplingSlotCount(query, histogramBucketCount)
	return &explorerSamplingState{
		startRealtimeUsec:    *query.AfterRealtimeUsec,
		endRealtimeUsec:      *query.BeforeRealtimeUsec,
		fileHeadRealtimeUsec: sampling.FileHeadRealtimeUsec,
		fileTailRealtimeUsec: sampling.FileTailRealtimeUsec,
		fileHeadSeqnum:       sampling.FileHeadSeqnum,
		fileTailSeqnum:       sampling.FileTailSeqnum,
		fileEntries:          sampling.FileEntries,
		stepRealtimeUsec:     explorerSamplingStep(*query.AfterRealtimeUsec, *query.BeforeRealtimeUsec, slots),
		enableAfterSamples:   sampling.Budget / 2,
		perFileEnableAfter:   explorerSamplingEnableAfter(sampling.Budget, maxU64(sampling.MatchedFiles, 1), query.Limit),
		perSlotEnableAfter:   explorerSamplingEnableAfter(sampling.Budget, uint64(slots), query.Limit),
		perSlotSampled:       make([]uint64, slots),
		perSlotUnsampled:     make([]uint64, slots),
		matchedFiles:         maxU64(sampling.MatchedFiles, 1),
		direction:            query.Direction,
	}
}

func explorerSamplingSlotCount(query ExplorerQuery, histogramBucketCount int) int {
	slots := histogramBucketCount
	if slots == 0 {
		slots = query.HistogramBuckets
	}
	if slots < 2 {
		return 2
	}
	if slots > explorerSamplingSlotsMax {
		return explorerSamplingSlotsMax
	}
	return slots
}

func explorerSamplingStep(after, before uint64, slots int) uint64 {
	step := saturatingSub(deltaValue(before, after)/uint64(slots), 1)
	if step == 0 {
		return 1
	}
	return step
}

func explorerSamplingEnableAfter(budget, divisor uint64, limit int) uint64 {
	value := (budget / 4) / maxU64(divisor, 1)
	if value < uint64(limit) {
		return uint64(limit)
	}
	return value
}

func (s *explorerSamplingState) beginFile(sampling ExplorerSampling) {
	if s == nil {
		return
	}
	s.fileHeadRealtimeUsec = sampling.FileHeadRealtimeUsec
	s.fileTailRealtimeUsec = sampling.FileTailRealtimeUsec
	s.fileHeadSeqnum = sampling.FileHeadSeqnum
	s.fileTailSeqnum = sampling.FileTailSeqnum
	s.fileEntries = sampling.FileEntries
	s.firstRealtimeUsec = nil
	s.perFileSampled = 0
	s.perFileUnsampled = 0
	s.perFileEvery = 0
	s.perFileSkipped = 0
	s.perFileRecalibrate = 0
}

func (s *explorerSamplingState) decide(realtimeUsec, seqnum uint64, candidateToKeep bool) explorerSamplingDecision {
	if s.firstRealtimeUsec == nil {
		value := realtimeUsec
		s.firstRealtimeUsec = &value
	}
	if candidateToKeep {
		return explorerSamplingDecision{kind: explorerSamplingFull}
	}
	slot := s.slotForRealtime(realtimeUsec)
	shouldSample := false
	if s.sampled < s.enableAfterSamples ||
		s.perFileSampled < s.perFileEnableAfter ||
		s.perSlotSampled[slot] < s.perSlotEnableAfter {
		shouldSample = true
	} else if s.perFileRecalibrate >= explorerSamplingRecalibrate || s.perFileEvery == 0 {
		s.recalibrate(realtimeUsec, seqnum)
		shouldSample = true
	} else if s.perFileSkipped >= s.perFileEvery {
		s.perFileSkipped = 0
		shouldSample = true
	} else {
		s.perFileSkipped++
	}
	if shouldSample {
		s.sampled++
		s.perFileSampled++
		s.perSlotSampled[slot]++
		return explorerSamplingDecision{kind: explorerSamplingFull, sampled: true}
	}
	s.perFileRecalibrate++
	s.perFileUnsampled++
	s.perSlotUnsampled[slot]++
	if s.perFileUnsampled > s.perFileSampled && s.progressByTime(realtimeUsec) > explorerSamplingEstimateAfter {
		remaining := s.estimateRemainingRows(realtimeUsec, seqnum)
		from, to := s.remainingRange(realtimeUsec)
		return explorerSamplingDecision{
			kind:             explorerSamplingStopAndEstimate,
			remainingRows:    remaining,
			fromRealtimeUsec: from,
			toRealtimeUsec:   to,
		}
	}
	return explorerSamplingDecision{kind: explorerSamplingSkipFields}
}

func (s *explorerSamplingState) slotForRealtime(realtimeUsec uint64) int {
	clamped := realtimeUsec
	if clamped < s.startRealtimeUsec {
		clamped = s.startRealtimeUsec
	}
	if clamped > s.endRealtimeUsec {
		clamped = s.endRealtimeUsec
	}
	slot := int(deltaValue(clamped, s.startRealtimeUsec) / maxU64(s.stepRealtimeUsec, 1))
	if slot >= len(s.perSlotSampled) {
		return len(s.perSlotSampled) - 1
	}
	if slot < 0 {
		return 0
	}
	return slot
}

func (s *explorerSamplingState) recalibrate(realtimeUsec, seqnum uint64) {
	remaining := s.estimateRemainingRows(realtimeUsec, seqnum)
	wanted := maxU64(s.enableAfterSamples/s.matchedFiles, 1)
	s.perFileEvery = maxU64(remaining/wanted, 1)
	s.perFileRecalibrate = 0
}

func (s *explorerSamplingState) estimateRemainingRows(realtimeUsec, seqnum uint64) uint64 {
	if remaining, ok := s.estimateRemainingRowsBySeqnum(seqnum); ok {
		return remaining
	}
	return s.estimateRemainingRowsByTime(realtimeUsec)
}

func (s *explorerSamplingState) estimateRemainingRowsBySeqnum(seqnum uint64) (uint64, bool) {
	if s.fileEntries == 0 || s.fileHeadSeqnum == 0 || s.fileTailSeqnum == 0 || seqnum == 0 {
		return 0, false
	}
	scanned := maxU64(s.perFileSampled+s.perFileUnsampled, 1)
	var span uint64
	switch s.direction {
	case DirectionBackward:
		if seqnum > s.fileTailSeqnum {
			return 0, false
		}
		span = s.fileTailSeqnum - seqnum
	default:
		if seqnum < s.fileHeadSeqnum {
			return 0, false
		}
		span = seqnum - s.fileHeadSeqnum
	}
	if span == 0 {
		return 0, false
	}
	proportion := float64(scanned) / float64(span)
	if proportion <= 0 {
		return 0, false
	}
	if proportion > 1 {
		proportion = 1
	}
	expected := uint64(proportion * float64(s.fileEntries))
	if expected == 0 {
		return 0, false
	}
	return maxU64(saturatingSub(expected, scanned), 1), true
}

func (s *explorerSamplingState) estimateRemainingRowsByTime(realtimeUsec uint64) uint64 {
	scanned := maxU64(s.perFileSampled+s.perFileUnsampled, 1)
	after, before := s.overlappingTimeframe(realtimeUsec)
	total, remaining, _, _ := s.remainingTimeDetails(realtimeUsec, after, before)
	total = maxU64(total, 1)
	elapsed := maxU64(saturatingSub(total, remaining), 1)
	proportion := float64(elapsed) / float64(total)
	if proportion == 0 || proportion > 1 {
		proportion = 1
	}
	expected := uint64(float64(scanned) / proportion)
	if s.fileEntries != 0 && expected > s.fileEntries {
		expected = s.fileEntries
	}
	return maxU64(saturatingSub(expected, scanned), 1)
}

func (s *explorerSamplingState) progressByTime(realtimeUsec uint64) float64 {
	after, before := s.overlappingTimeframe(realtimeUsec)
	total := maxU64(saturatingSub(before, after), 1)
	var elapsed uint64
	switch s.direction {
	case DirectionBackward:
		elapsed = minUint64(saturatingSub(before, realtimeUsec), total)
	default:
		elapsed = minUint64(saturatingSub(realtimeUsec, after), total)
	}
	return float64(elapsed) / float64(total)
}

func (s *explorerSamplingState) remainingRange(realtimeUsec uint64) (uint64, uint64) {
	after, before := s.overlappingTimeframe(realtimeUsec)
	_, _, start, end := s.remainingTimeDetails(realtimeUsec, after, before)
	return start, end
}

func (s *explorerSamplingState) overlappingTimeframe(realtimeUsec uint64) (uint64, uint64) {
	switch s.direction {
	case DirectionBackward:
		newest := s.endRealtimeUsec
		if s.fileTailRealtimeUsec != 0 {
			newest = s.fileTailRealtimeUsec
		}
		if s.firstRealtimeUsec != nil {
			newest = *s.firstRealtimeUsec
		}
		oldest := s.startRealtimeUsec
		if s.fileHeadRealtimeUsec != 0 {
			oldest = maxU64(s.startRealtimeUsec, s.fileHeadRealtimeUsec)
		}
		if newest <= oldest {
			newest = saturatingAdd(oldest, 1)
		}
		if newest < realtimeUsec {
			newest = saturatingAdd(realtimeUsec, 1)
		}
		return oldest, newest
	default:
		oldest := s.startRealtimeUsec
		if s.fileHeadRealtimeUsec != 0 {
			oldest = s.fileHeadRealtimeUsec
		}
		if s.firstRealtimeUsec != nil {
			oldest = *s.firstRealtimeUsec
		}
		newest := s.endRealtimeUsec
		if s.fileTailRealtimeUsec != 0 {
			newest = minUint64(s.endRealtimeUsec, s.fileTailRealtimeUsec)
		}
		if newest <= oldest {
			newest = saturatingAdd(oldest, 1)
		}
		if realtimeUsec < oldest {
			oldest = saturatingSub(realtimeUsec, 1)
		}
		return oldest, newest
	}
}

func (s *explorerSamplingState) remainingTimeDetails(realtimeUsec, after, before uint64) (uint64, uint64, uint64, uint64) {
	if realtimeUsec <= after {
		after = saturatingSub(realtimeUsec, 1)
	}
	if realtimeUsec >= before {
		before = saturatingAdd(realtimeUsec, 1)
	}
	if before <= after {
		before = saturatingAdd(after, 1)
	}
	var start, end uint64
	switch s.direction {
	case DirectionBackward:
		start, end = after, realtimeUsec
	default:
		start, end = realtimeUsec, before
	}
	return maxU64(deltaValue(before, after), 1), deltaValue(end, start), start, end
}

func (r *Reader) Explore(query ExplorerQuery) (ExplorerResult, error) {
	return r.ExploreWithStrategy(query, ExplorerStrategyTraversal)
}

func (r *Reader) ExploreWithStrategy(query ExplorerQuery, strategy ExplorerStrategy) (ExplorerResult, error) {
	return r.exploreWithPayloadMode(query, strategy, explorerRowPayloadExpand, nil, true)
}

func (r *Reader) ExploreWithStrategyAndControl(query ExplorerQuery, strategy ExplorerStrategy, control *ExplorerControl) (ExplorerResult, error) {
	return r.exploreWithPayloadMode(query, strategy, explorerRowPayloadExpand, control, true)
}

func (r *Reader) exploreCursorRows(query ExplorerQuery, strategy ExplorerStrategy, control *ExplorerControl) (ExplorerResult, error) {
	return r.exploreWithPayloadMode(query, strategy, explorerRowPayloadCursorOnly, control, true)
}

func (r *Reader) exploreWithPayloadMode(query ExplorerQuery, strategy ExplorerStrategy, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl, rejectDebug bool) (ExplorerResult, error) {
	if rejectDebug && query.DebugCollectColumnFieldsByRowTraversal {
		return ExplorerResult{}, errExplorerDebugDisabled
	}
	switch strategy {
	case ExplorerStrategyTraversal:
		return r.exploreTraversal(query, rowPayloadMode, control)
	case ExplorerStrategyIndex:
		return r.exploreIndexed(query, rowPayloadMode, control)
	case ExplorerStrategyCompare:
		return r.exploreCompare(query, rowPayloadMode)
	default:
		return ExplorerResult{}, fmt.Errorf("%w: unsupported explorer strategy %d", ErrUnsupported, strategy)
	}
}

func (r *Reader) exploreTraversal(query ExplorerQuery, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl) (ExplorerResult, error) {
	if err := validateExplorerQuery(query); err != nil {
		return ExplorerResult{}, err
	}
	result, err := r.explorerResultForQuery(query)
	if err != nil {
		return ExplorerResult{}, err
	}
	groups := facetPassGroups(query)
	if canRunCombinedExplorerPass(groups) {
		if err := r.exploreTraversalCombined(query, groups, rowPayloadMode, control, &result); err != nil {
			return ExplorerResult{}, err
		}
		return result, nil
	}
	if err := r.exploreTraversalMain(query, rowPayloadMode, control, &result); err != nil {
		return ExplorerResult{}, err
	}
	if err := r.exploreTraversalFacetGroups(query, groups, control, &result); err != nil {
		return ExplorerResult{}, err
	}
	return result, nil
}

func (r *Reader) exploreTraversalCombined(query ExplorerQuery, groups []facetPassGroup, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl, result *ExplorerResult) error {
	indices := combinedFacetIndices(groups)
	if !queryNeedsMainPass(query) && len(indices) == 0 {
		return nil
	}
	candidates, err := r.explorerCandidateSet(query, nil, false)
	if err != nil {
		return err
	}
	acc := newExplorerAccumulator(query, indices, result.Histogram)
	if err := r.scanExplorerCombined(query, candidates, acc, result, len(indices) != 0, rowPayloadMode, control); err != nil {
		return err
	}
	acc.finishFacets(result)
	acc.finishHistogram(result.Histogram)
	return nil
}

func (r *Reader) exploreTraversalMain(query ExplorerQuery, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl, result *ExplorerResult) error {
	if !queryNeedsMainPass(query) {
		return nil
	}
	candidates, err := r.explorerCandidateSet(query, nil, false)
	if err != nil {
		return err
	}
	acc := newExplorerAccumulator(query, nil, result.Histogram)
	if err := r.scanExplorerMain(query, candidates, acc, result, rowPayloadMode, control); err != nil {
		return err
	}
	acc.finishHistogram(result.Histogram)
	return nil
}

func (r *Reader) exploreTraversalFacetGroups(query ExplorerQuery, groups []facetPassGroup, control *ExplorerControl, result *ExplorerResult) error {
	for _, group := range groups {
		if explorerStopped(control) {
			return nil
		}
		if err := r.exploreTraversalFacetGroup(query, group, control, result); err != nil {
			return err
		}
	}
	return nil
}

func (r *Reader) exploreTraversalFacetGroup(query ExplorerQuery, group facetPassGroup, control *ExplorerControl, result *ExplorerResult) error {
	candidates, err := r.explorerCandidateSet(query, group.excludedField, false)
	if err != nil {
		return err
	}
	acc := newExplorerFacetAccumulator(query, group.facetIndices, facetPassNeedsSourceRealtime(query))
	if err := r.scanExplorerFacet(query, candidates, acc, &result.Stats, control); err != nil {
		return err
	}
	acc.finishFacets(result)
	return nil
}

func (r *Reader) exploreCompare(query ExplorerQuery, rowPayloadMode explorerRowPayloadMode) (ExplorerResult, error) {
	started := time.Now()
	traversal, err := r.exploreTraversal(query, rowPayloadMode, nil)
	if err != nil {
		return ExplorerResult{}, err
	}
	traversalDuration := time.Since(started)

	started = time.Now()
	indexed, err := r.exploreIndexed(query, rowPayloadMode, nil)
	if err != nil {
		return ExplorerResult{}, err
	}
	indexDuration := time.Since(started)

	if !explorerOutputsMatch(traversal, indexed) {
		return ExplorerResult{}, errExplorerVerification
	}
	indexed.Comparison = &ExplorerComparison{
		TraversalDuration: traversalDuration,
		IndexDuration:     indexDuration,
		TraversalStats:    traversal.Stats,
		IndexStats:        indexed.Stats,
	}
	return indexed, nil
}

func (r *Reader) exploreIndexed(query ExplorerQuery, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl) (ExplorerResult, error) {
	if err := validateExplorerQuery(query); err != nil {
		return ExplorerResult{}, err
	}
	if err := validateIndexedExplorerQuery(query); err != nil {
		return ExplorerResult{}, err
	}
	result, err := r.explorerResultForQuery(query)
	if err != nil {
		return ExplorerResult{}, err
	}
	if err := r.exploreIndexedRows(query, rowPayloadMode, control, &result); err != nil {
		return ExplorerResult{}, err
	}
	if err := r.exploreIndexedFacets(query, control, &result); err != nil {
		return ExplorerResult{}, err
	}
	if err := r.exploreIndexedHistogram(query, control, &result); err != nil {
		return ExplorerResult{}, err
	}
	return result, nil
}

func (r *Reader) exploreIndexedRows(query ExplorerQuery, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl, result *ExplorerResult) error {
	if query.Limit == 0 {
		return nil
	}
	rowQuery := query
	rowQuery.Facets = nil
	rowQuery.Histogram = nil
	candidates, err := r.explorerCandidateSet(rowQuery, nil, true)
	if err != nil {
		return err
	}
	acc := newExplorerAccumulator(rowQuery, nil, nil)
	return r.scanExplorerMain(rowQuery, candidates, acc, result, rowPayloadMode, control)
}

func (r *Reader) exploreIndexedFacets(query ExplorerQuery, control *ExplorerControl, result *ExplorerResult) error {
	if explorerStopped(control) {
		return nil
	}
	for _, group := range facetPassGroups(query) {
		candidates, err := r.explorerCandidateSet(query, group.excludedField, true)
		if err != nil {
			return err
		}
		if err := r.indexedCountFacetGroup(query, group, candidates, result); err != nil {
			return err
		}
	}
	return nil
}

func (r *Reader) exploreIndexedHistogram(query ExplorerQuery, control *ExplorerControl, result *ExplorerResult) error {
	if query.Histogram == nil || explorerStopped(control) {
		return nil
	}
	candidates, err := r.explorerCandidateSet(query, nil, true)
	if err != nil {
		return err
	}
	return r.indexedCountHistogram(query, candidates, result)
}

func explorerStopped(control *ExplorerControl) bool {
	return control != nil && control.StopReason() != ExplorerStopNone
}

type explorerCandidateSet struct {
	all     bool
	count   uint64
	offsets map[uint64]struct{}
	ordered []uint64
	cursor  int
}

func (s explorerCandidateSet) contains(offset uint64) bool {
	if s.all {
		return true
	}
	_, ok := s.offsets[offset]
	return ok
}

func newExplorerCandidateSet(offsets map[uint64]struct{}) explorerCandidateSet {
	ordered := make([]uint64, 0, len(offsets))
	for offset := range offsets {
		ordered = append(ordered, offset)
	}
	sort.Slice(ordered, func(i, j int) bool { return ordered[i] < ordered[j] })
	return explorerCandidateSet{offsets: offsets, ordered: ordered, count: uint64(len(ordered))}
}

func (s *explorerCandidateSet) prepare(direction Direction) {
	if s == nil || s.all {
		return
	}
	if direction == DirectionBackward {
		s.cursor = len(s.ordered) - 1
	} else {
		s.cursor = 0
	}
}

func (s *explorerCandidateSet) nextOffset(direction Direction) (uint64, bool) {
	if s == nil || s.all {
		return 0, false
	}
	if direction == DirectionBackward {
		if s.cursor < 0 || len(s.ordered) == 0 {
			return 0, false
		}
		offset := s.ordered[s.cursor]
		s.cursor--
		return offset, true
	}
	if s.cursor >= len(s.ordered) {
		return 0, false
	}
	offset := s.ordered[s.cursor]
	s.cursor++
	return offset, true
}

func (r *Reader) explorerCandidateSet(query ExplorerQuery, excludedField []byte, includeTimeFilter bool) (explorerCandidateSet, error) {
	active := activeExplorerFilters(query.Filters, excludedField)
	needsTimeFilter := includeTimeFilter && (query.AfterRealtimeUsec != nil || query.BeforeRealtimeUsec != nil)
	if len(active) == 0 && !needsTimeFilter {
		return explorerCandidateSet{all: true, count: uint64(len(r.entryOffsets))}, nil
	}
	current, err := r.entryOffsetsForFilters(active)
	if err != nil {
		return explorerCandidateSet{}, err
	}
	if current == nil {
		current = r.allEntryOffsetSet()
	}
	if needsTimeFilter {
		if err := r.applyCandidateTimeFilter(query, current); err != nil {
			return explorerCandidateSet{}, err
		}
	}
	return newExplorerCandidateSet(current), nil
}

func activeExplorerFilters(filters []ExplorerFilter, excludedField []byte) []ExplorerFilter {
	active := make([]ExplorerFilter, 0, len(filters))
	for _, filter := range filters {
		if len(filter.Values) == 0 {
			continue
		}
		if excludedField != nil && bytes.Equal(filter.Field, excludedField) {
			continue
		}
		active = append(active, filter)
	}
	return active
}

func (r *Reader) entryOffsetsForFilters(filters []ExplorerFilter) (map[uint64]struct{}, error) {
	var current map[uint64]struct{}
	for _, filter := range filters {
		fieldSet, err := r.entryOffsetsForFilter(filter)
		if err != nil {
			return nil, err
		}
		current = intersectEntryOffsetSets(current, fieldSet)
		if len(current) == 0 {
			break
		}
	}
	return current, nil
}

func (r *Reader) entryOffsetsForFilter(filter ExplorerFilter) (map[uint64]struct{}, error) {
	fieldSet := make(map[uint64]struct{})
	for _, value := range filter.Values {
		if err := r.addEntryOffsetsForFilterValue(fieldSet, filter.Field, value); err != nil {
			return nil, err
		}
	}
	return fieldSet, nil
}

func (r *Reader) addEntryOffsetsForFilterValue(fieldSet map[uint64]struct{}, field, value []byte) error {
	offset, ok, err := r.findDataOffsetByPayload(payloadFromParts(field, value))
	if err != nil || !ok {
		return err
	}
	header, err := r.readDataHeaderAt(offset)
	if err != nil {
		return err
	}
	return r.visitDataEntryOffsets(header, func(entryOffset uint64) error {
		fieldSet[entryOffset] = struct{}{}
		return nil
	})
}

func intersectEntryOffsetSets(current, fieldSet map[uint64]struct{}) map[uint64]struct{} {
	if current == nil {
		return fieldSet
	}
	for offset := range current {
		if _, ok := fieldSet[offset]; !ok {
			delete(current, offset)
		}
	}
	return current
}

func (r *Reader) allEntryOffsetSet() map[uint64]struct{} {
	current := make(map[uint64]struct{}, len(r.entryOffsets))
	for _, offset := range r.entryOffsets {
		current[offset] = struct{}{}
	}
	return current
}

func (r *Reader) applyCandidateTimeFilter(query ExplorerQuery, current map[uint64]struct{}) error {
	for offset := range current {
		header, err := r.readEntryHeaderAt(offset)
		if err != nil {
			return err
		}
		if !timestampInRange(query, header.realtime) {
			delete(current, offset)
		}
	}
	return nil
}

func (r *Reader) findDataOffsetByPayload(payload []byte) (uint64, bool, error) {
	if r.header.dataHashTableOffset == 0 || r.header.dataHashTableSize < hashItemSize {
		return 0, false, nil
	}
	hash := r.hash(payload)
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
			stored, err := r.readDataPayloadWithHeader(offset, header)
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

func (r *Reader) readDataPayloadWithHeader(offset uint64, header dataHeader) ([]byte, error) {
	payloadOffset := r.dataPayloadOffset()
	if header.object.typ != objectTypeData || header.object.size < payloadOffset {
		return nil, errCorruptObject
	}
	payloadLen := header.object.size - payloadOffset
	payload, err := r.readSlice(offset+payloadOffset, payloadLen)
	if err != nil {
		return nil, err
	}
	return decompressDataPayload(header.object.flag, payload)
}

func (r *Reader) firstDataEntryOffset(header dataHeader) (uint64, bool, error) {
	if header.nEntries == 0 {
		return 0, false, nil
	}
	if header.entryOffset != 0 {
		return header.entryOffset, true, nil
	}
	if header.entryArrayOffset == 0 {
		return 0, false, nil
	}
	_, offsets, _, err := r.readEntryArrayChunk(header.entryArrayOffset, 1)
	if err != nil {
		return 0, false, err
	}
	if len(offsets) == 0 {
		return 0, false, nil
	}
	return offsets[0], true, nil
}

func (r *Reader) visitDataEntryOffsets(header dataHeader, visit func(uint64) error) error {
	if header.nEntries == 0 {
		return nil
	}
	if header.entryOffset != 0 {
		if err := visit(header.entryOffset); err != nil {
			return err
		}
	}
	remaining := header.nEntries - 1
	offset := header.entryArrayOffset
	for offset != 0 && remaining > 0 {
		arrayHeader, chunkOffsets, used, err := r.readEntryArrayChunk(offset, remaining)
		if err != nil {
			return err
		}
		for _, entryOffset := range chunkOffsets {
			if err := visit(entryOffset); err != nil {
				return err
			}
		}
		remaining -= used
		offset = arrayHeader.nextArrayOffset
	}
	return nil
}

func (r *Reader) visitFieldDataObjects(field []byte, visit func(uint64, dataHeader, []byte) error) error {
	offset, ok, err := r.findFieldHeadDataOffset(field)
	if err != nil || !ok {
		return err
	}
	for offset != 0 {
		header, err := r.readDataHeaderAt(offset)
		if err != nil {
			return err
		}
		payload, err := r.readDataPayloadWithHeader(offset, header)
		if err != nil {
			return err
		}
		if err := visit(offset, header, payload); err != nil {
			return err
		}
		offset = header.nextFieldOffset
	}
	return nil
}

func (r *Reader) indexedCountFacetGroup(query ExplorerQuery, group facetPassGroup, candidates explorerCandidateSet, result *ExplorerResult) error {
	result.Stats.FacetRowsMatched += candidates.count
	for _, facetIndex := range group.facetIndices {
		if facetIndex < 0 || facetIndex >= len(query.Facets) {
			continue
		}
		field := query.Facets[facetIndex]
		values := make(map[string]uint64)
		rowsWithField := make(map[uint64]struct{})
		err := r.visitFieldDataObjects(field, func(_ uint64, header dataHeader, payload []byte) error {
			payloadField, value, ok := splitRawPayload(payload)
			if !ok || !bytes.Equal(payloadField, field) {
				return nil
			}
			var count uint64
			if err := r.visitDataEntryOffsets(header, func(entryOffset uint64) error {
				result.Stats.DataRefsSeen++
				if candidates.contains(entryOffset) {
					count++
					rowsWithField[entryOffset] = struct{}{}
				}
				return nil
			}); err != nil {
				return err
			}
			if count != 0 {
				values[string(value)] += count
				result.Stats.FacetUpdates += count
			}
			result.Stats.DataObjectsClassified++
			result.Stats.DataPayloadsLoaded++
			if header.object.flag&(objectCompressedXZ|objectCompressedLZ4|objectCompressedZSTD) != 0 {
				result.Stats.PayloadsDecompressed++
			}
			return nil
		})
		if err != nil {
			return err
		}
		if unset := candidates.count - uint64(len(rowsWithField)); unset != 0 {
			values[string(explorerUnsetValue)] += unset
			result.Stats.FacetUpdates += unset
		}
		result.Facets[string(field)] = values
	}
	return nil
}

func (r *Reader) indexedCountHistogram(query ExplorerQuery, candidates explorerCandidateSet, result *ExplorerResult) error {
	if result.Histogram == nil || query.Histogram == nil {
		return nil
	}
	field := result.Histogram.Field
	rowsWithField := make(map[uint64]struct{})
	err := r.visitFieldDataObjects(field, func(_ uint64, header dataHeader, payload []byte) error {
		payloadField, value, ok := splitRawPayload(payload)
		if !ok || !bytes.Equal(payloadField, field) {
			return nil
		}
		if err := r.visitDataEntryOffsets(header, func(entryOffset uint64) error {
			result.Stats.DataRefsSeen++
			if !candidates.contains(entryOffset) {
				return nil
			}
			entryHeader, err := r.readEntryHeaderAt(entryOffset)
			if err != nil {
				return err
			}
			rowsWithField[entryOffset] = struct{}{}
			if !timestampInRange(query, entryHeader.realtime) {
				return nil
			}
			bucket := histogramBucketIndex(result.Histogram, entryHeader.realtime)
			if bucket >= 0 {
				result.Histogram.Buckets[bucket].Values[string(value)]++
				result.Stats.HistogramUpdates++
			}
			return nil
		}); err != nil {
			return err
		}
		result.Stats.DataObjectsClassified++
		result.Stats.DataPayloadsLoaded++
		if header.object.flag&(objectCompressedXZ|objectCompressedLZ4|objectCompressedZSTD) != 0 {
			result.Stats.PayloadsDecompressed++
		}
		return nil
	})
	if err != nil {
		return err
	}
	for _, entryOffset := range r.candidateOffsets(candidates) {
		if _, ok := rowsWithField[entryOffset]; ok {
			continue
		}
		entryHeader, err := r.readEntryHeaderAt(entryOffset)
		if err != nil {
			return err
		}
		if !timestampInRange(query, entryHeader.realtime) {
			continue
		}
		bucket := histogramBucketIndex(result.Histogram, entryHeader.realtime)
		if bucket >= 0 {
			result.Histogram.Buckets[bucket].Values[string(explorerUnsetValue)]++
			result.Stats.HistogramUpdates++
		}
	}
	return nil
}

func (r *Reader) candidateOffsets(candidates explorerCandidateSet) []uint64 {
	if candidates.all {
		return r.entryOffsets
	}
	return candidates.ordered
}

type rowScan struct {
	timestamp        *uint64
	ftsMatches       bool
	ftsNegativeMatch bool
}

type offsetClassKind int

const (
	offsetClassIrrelevant offsetClassKind = iota
	offsetClassFtsMatch
	offsetClassFtsNegative
	offsetClassValue
)

type offsetClass struct {
	kind       offsetClassKind
	valueIndex int
}

const (
	offsetClassIrrelevantRaw = 1
	offsetClassFtsMatchRaw   = 2
	offsetClassFtsNegRaw     = 3
	offsetClassValueBase     = 4
)

func (c offsetClass) raw() int {
	switch c.kind {
	case offsetClassFtsMatch:
		return offsetClassFtsMatchRaw
	case offsetClassFtsNegative:
		return offsetClassFtsNegRaw
	case offsetClassValue:
		return offsetClassValueBase + c.valueIndex
	default:
		return offsetClassIrrelevantRaw
	}
}

func offsetClassFromRaw(raw int) offsetClass {
	switch raw {
	case offsetClassIrrelevantRaw:
		return offsetClass{kind: offsetClassIrrelevant}
	case offsetClassFtsMatchRaw:
		return offsetClass{kind: offsetClassFtsMatch}
	case offsetClassFtsNegRaw:
		return offsetClass{kind: offsetClassFtsNegative}
	default:
		return offsetClass{kind: offsetClassValue, valueIndex: raw - offsetClassValueBase}
	}
}

type offsetClassSlot struct {
	offset   uint64
	classRaw int
}

type offsetClassCache struct {
	slots []offsetClassSlot
	len   int
}

func newOffsetClassCache() offsetClassCache {
	return offsetClassCache{slots: make([]offsetClassSlot, 256)}
}

func (c *offsetClassCache) lookup(offset uint64) (offsetClass, bool) {
	if len(c.slots) == 0 {
		return offsetClass{}, false
	}
	mask := len(c.slots) - 1
	index := offsetSlot(offset) & mask
	for range c.slots {
		slot := c.slots[index]
		if slot.offset == 0 {
			return offsetClass{}, false
		}
		if slot.offset == offset {
			return offsetClassFromRaw(slot.classRaw), true
		}
		index = (index + 1) & mask
	}
	return offsetClass{}, false
}

func (c *offsetClassCache) insert(offset uint64, class offsetClass) {
	if offset == 0 {
		return
	}
	if len(c.slots) == 0 {
		c.slots = make([]offsetClassSlot, 256)
	}
	if (c.len+1)*4 >= len(c.slots)*3 {
		c.grow()
	}
	c.insertRaw(offset, class.raw())
}

func (c *offsetClassCache) grow() {
	old := c.slots
	c.slots = make([]offsetClassSlot, maxInt(len(old)*2, 256))
	c.len = 0
	for _, slot := range old {
		if slot.offset != 0 {
			c.insertRaw(slot.offset, slot.classRaw)
		}
	}
}

func (c *offsetClassCache) insertRaw(offset uint64, classRaw int) {
	mask := len(c.slots) - 1
	index := offsetSlot(offset) & mask
	for {
		if c.slots[index].offset == 0 {
			c.slots[index] = offsetClassSlot{offset: offset, classRaw: classRaw}
			c.len++
			return
		}
		if c.slots[index].offset == offset {
			c.slots[index].classRaw = classRaw
			return
		}
		index = (index + 1) & mask
	}
}

func offsetSlot(offset uint64) int {
	value := offset >> 3
	value ^= value >> 33
	value *= 0xff51afd7ed558ccd
	value ^= value >> 33
	return int(value)
}

type explorerAccumulator struct {
	fieldLookup               map[string]int
	fields                    [][]byte
	flags                     []uint8
	lastSeenRowIDs            []uint64
	fieldSeenRows             []uint64
	unsetCounts               []uint64
	facetRowsMatched          uint64
	rowPublicFieldIndices     []int
	valuesByField             [][]int
	valueCounts               []uint64
	valueFieldIndices         []int
	valueLabels               [][]byte
	valueFTSMatches           []bool
	valueSourceRealtime       []*uint64
	valueHistogramBuckets     [][]uint64
	fieldHistogramUnsetBucket [][]uint64
	offsetCache               offsetClassCache
	histogramStartUsec        uint64
	histogramBucketWidthUsec  uint64
	histogramBucketCount      int
	requiredIdentityCount     int
}

func newExplorerAccumulator(query ExplorerQuery, facetIndices []int, histogram *ExplorerHistogram) *explorerAccumulator {
	out := newExplorerAccumulatorBase(histogram)
	if query.Histogram != nil {
		out.addField(query.Histogram, facetHistogram)
	}
	for _, index := range facetIndices {
		if index >= 0 && index < len(query.Facets) {
			out.addField(query.Facets[index], facetPublic)
		}
	}
	if queryNeedsSourceRealtimeMain(query) || facetPassNeedsSourceRealtime(query) {
		out.addField(sourceRealtimeField, facetSourceRealtime)
	}
	return out
}

func newExplorerFacetAccumulator(query ExplorerQuery, facetIndices []int, includeSourceRealtime bool) *explorerAccumulator {
	out := newExplorerAccumulatorBase(nil)
	for _, index := range facetIndices {
		if index >= 0 && index < len(query.Facets) {
			out.addField(query.Facets[index], facetPublic)
		}
	}
	if includeSourceRealtime {
		out.addField(sourceRealtimeField, facetSourceRealtime)
	}
	return out
}

func newExplorerAccumulatorBase(histogram *ExplorerHistogram) *explorerAccumulator {
	start := uint64(0)
	width := uint64(1)
	count := 0
	if histogram != nil && len(histogram.Buckets) != 0 {
		start = histogram.Buckets[0].StartRealtimeUsec
		width = histogram.Buckets[0].EndRealtimeUsec - histogram.Buckets[0].StartRealtimeUsec
		if width == 0 {
			width = 1
		}
		count = len(histogram.Buckets)
	}
	return &explorerAccumulator{
		fieldLookup:              make(map[string]int),
		offsetCache:              newOffsetClassCache(),
		histogramStartUsec:       start,
		histogramBucketWidthUsec: width,
		histogramBucketCount:     count,
	}
}

func (a *explorerAccumulator) addField(field []byte, flags uint8) {
	key := string(field)
	if index, ok := a.fieldLookup[key]; ok {
		hadRequired := a.flags[index] != 0
		a.flags[index] |= flags
		if flags&facetHistogram != 0 && a.fieldHistogramUnsetBucket[index] == nil {
			a.fieldHistogramUnsetBucket[index] = make([]uint64, a.histogramBucketCount)
		}
		if !hadRequired && a.flags[index] != 0 {
			a.requiredIdentityCount++
		}
		return
	}
	index := len(a.fields)
	a.fieldLookup[key] = index
	a.fields = append(a.fields, cloneBytes(field))
	a.flags = append(a.flags, flags)
	a.lastSeenRowIDs = append(a.lastSeenRowIDs, 0)
	a.fieldSeenRows = append(a.fieldSeenRows, 0)
	a.unsetCounts = append(a.unsetCounts, 0)
	a.valuesByField = append(a.valuesByField, nil)
	if flags&facetHistogram != 0 {
		a.fieldHistogramUnsetBucket = append(a.fieldHistogramUnsetBucket, make([]uint64, a.histogramBucketCount))
	} else {
		a.fieldHistogramUnsetBucket = append(a.fieldHistogramUnsetBucket, nil)
	}
	if flags != 0 {
		a.requiredIdentityCount++
	}
}

func (a *explorerAccumulator) addValue(fieldIndex int, value []byte, ftsMatches bool) int {
	valueIndex := len(a.valueCounts)
	flags := a.flags[fieldIndex]
	a.valueCounts = append(a.valueCounts, 0)
	a.valueFieldIndices = append(a.valueFieldIndices, fieldIndex)
	a.valueLabels = append(a.valueLabels, cloneBytes(value))
	a.valueFTSMatches = append(a.valueFTSMatches, ftsMatches)
	if flags&facetSourceRealtime != 0 {
		parsed := parseSourceRealtime(value)
		a.valueSourceRealtime = append(a.valueSourceRealtime, parsed)
	} else {
		a.valueSourceRealtime = append(a.valueSourceRealtime, nil)
	}
	if flags&facetHistogram != 0 {
		a.valueHistogramBuckets = append(a.valueHistogramBuckets, make([]uint64, a.histogramBucketCount))
	} else {
		a.valueHistogramBuckets = append(a.valueHistogramBuckets, nil)
	}
	a.valuesByField[fieldIndex] = append(a.valuesByField[fieldIndex], valueIndex)
	return valueIndex
}

func (a *explorerAccumulator) markFieldSeen(fieldIndex int, rowID uint64) bool {
	if a.lastSeenRowIDs[fieldIndex] == rowID {
		return false
	}
	a.lastSeenRowIDs[fieldIndex] = rowID
	return true
}

func (a *explorerAccumulator) applyValue(valueIndex int, realtimeUsec *uint64, stats *ExplorerStats) {
	fieldIndex := a.valueFieldIndices[valueIndex]
	flags := a.flags[fieldIndex]
	if flags&facetPublic != 0 {
		a.valueCounts[valueIndex]++
		stats.FacetUpdates++
	}
	if flags&facetHistogram != 0 && realtimeUsec != nil {
		buckets := a.valueHistogramBuckets[valueIndex]
		if bucket := histogramBucketIndexFromBounds(*realtimeUsec, a.histogramStartUsec, a.histogramBucketWidthUsec, len(buckets)); bucket >= 0 {
			buckets[bucket]++
			stats.HistogramUpdates++
		}
	}
}

func (a *explorerAccumulator) finishFacetRow(rowID uint64, stats *ExplorerStats) {
	_ = stats
	a.facetRowsMatched++
	for _, fieldIndex := range a.rowPublicFieldIndices {
		if a.lastSeenRowIDs[fieldIndex] == rowID {
			a.fieldSeenRows[fieldIndex]++
		}
	}
}

func (a *explorerAccumulator) finishHistogramRow(rowID uint64, realtimeUsec uint64, stats *ExplorerStats) {
	for fieldIndex := range a.fields {
		if a.flags[fieldIndex]&facetHistogram == 0 || a.lastSeenRowIDs[fieldIndex] == rowID {
			continue
		}
		buckets := a.fieldHistogramUnsetBucket[fieldIndex]
		if bucket := histogramBucketIndexFromBounds(realtimeUsec, a.histogramStartUsec, a.histogramBucketWidthUsec, len(buckets)); bucket >= 0 {
			buckets[bucket]++
			stats.HistogramUpdates++
		}
	}
}

func (a *explorerAccumulator) finishFacets(result *ExplorerResult) {
	for fieldIndex, field := range a.fields {
		if a.flags[fieldIndex]&facetPublic == 0 {
			continue
		}
		if a.facetRowsMatched > a.fieldSeenRows[fieldIndex] {
			unset := a.facetRowsMatched - a.fieldSeenRows[fieldIndex]
			a.unsetCounts[fieldIndex] += unset
			result.Stats.FacetUpdates += unset
		}
		values := make(map[string]uint64)
		for _, valueIndex := range a.valuesByField[fieldIndex] {
			if count := a.valueCounts[valueIndex]; count != 0 {
				values[string(a.valueLabels[valueIndex])] += count
			}
		}
		if a.unsetCounts[fieldIndex] != 0 {
			values[string(explorerUnsetValue)] += a.unsetCounts[fieldIndex]
		}
		result.Facets[string(field)] = values
	}
}

func (a *explorerAccumulator) finishHistogram(histogram *ExplorerHistogram) {
	if histogram == nil {
		return
	}
	for _, buckets := range a.fieldHistogramUnsetBucket {
		for bucketIndex, count := range buckets {
			if count == 0 {
				continue
			}
			histogram.Buckets[bucketIndex].Values[string(explorerUnsetValue)] += count
		}
	}
	for valueIndex, buckets := range a.valueHistogramBuckets {
		for bucketIndex, count := range buckets {
			if count == 0 {
				continue
			}
			histogram.Buckets[bucketIndex].Values[string(a.valueLabels[valueIndex])] += count
		}
	}
}

func (r *Reader) scanExplorerMain(query ExplorerQuery, candidates explorerCandidateSet, acc *explorerAccumulator, result *ExplorerResult, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl) error {
	r.seekForExplorer(query)
	candidates.prepare(query.Direction)
	var rowID, rowsSeen uint64
	var deferred []int
	needsFTS := queryHasFTS(query)
	for {
		frame, ok, stop, err := r.nextExplorerRowFrame(query, &candidates, &rowsSeen, result.Stats, control)
		if err != nil || stop {
			return err
		}
		if !ok {
			break
		}
		scan, err := r.scanRowDataOrDefault(query, acc, &rowID, &deferred, &result.Stats, needsFTS)
		if err != nil {
			return err
		}
		effective, accepted := acceptedEffectiveRealtime(query, scan, frame.commitRealtime, &result.Stats, control)
		if !accepted {
			continue
		}
		recordLastRealtime(&result.Stats, frame.commitRealtime)
		result.Stats.RowsMatched++
		if control != nil && control.emitMatchedRow(effective, result.Stats.RowsMatched) {
			break
		}
		for _, valueIndex := range deferred {
			acc.applyValue(valueIndex, &effective, &result.Stats)
		}
		acc.finishHistogramRow(rowID, effective, &result.Stats)
		if err := r.pushExplorerRowIfWanted(query, result, rowPayloadMode, effective); err != nil {
			return err
		}
		if shouldStopWhenRowsFull(query, result.Rows, effective, result.Stats.RowsMatched) {
			break
		}
	}
	result.Stats.RowsReturned = uint64(len(result.Rows))
	return nil
}

func (r *Reader) scanExplorerCombined(query ExplorerQuery, candidates explorerCandidateSet, acc *explorerAccumulator, result *ExplorerResult, includeFacets bool, rowPayloadMode explorerRowPayloadMode, control *ExplorerControl) error {
	r.seekForExplorer(query)
	candidates.prepare(query.Direction)
	mode := combinedScanMode{includeMain: queryNeedsMainPass(query), includeFacets: includeFacets}
	var rowID, rowsSeen uint64
	var deferred []int
	sampling := samplingStateForCombined(query, result, control)
	needsFTS := queryHasFTS(query)
	for {
		frame, ok, stop, err := r.nextExplorerRowFrame(query, &candidates, &rowsSeen, result.Stats, control)
		if err != nil || stop {
			return err
		}
		if !ok {
			break
		}
		action := applyCombinedSamplingIfNeeded(query, mode, sampling, control, result, frame)
		if action == samplingRowSkip {
			continue
		}
		if action == samplingRowStop {
			break
		}
		scan, err := r.scanRowDataOrDefault(query, acc, &rowID, &deferred, &result.Stats, needsFTS)
		if err != nil {
			return err
		}
		effective, accepted := acceptedEffectiveRealtime(query, scan, frame.commitRealtime, &result.Stats, control)
		if !accepted {
			continue
		}
		recordLastRealtime(&result.Stats, frame.commitRealtime)
		stopAfterMatched := finishCombinedExplorerRow(query, mode, acc, result, rowID, effective, deferred, control)
		if err := r.pushExplorerRowIfWanted(query, result, rowPayloadMode, effective); err != nil {
			return err
		}
		if stopAfterMatched || shouldStopWhenRowsFull(query, result.Rows, effective, result.Stats.RowsMatched) {
			break
		}
	}
	result.Stats.RowsReturned = uint64(len(result.Rows))
	return nil
}

func applyCombinedSamplingIfNeeded(query ExplorerQuery, mode combinedScanMode, sampling *explorerSamplingState, control *ExplorerControl, result *ExplorerResult, frame explorerRowFrame) samplingRowAction {
	decision, ok := combinedSamplingDecision(query, result.Rows, frame, sampling, control)
	if !ok {
		return samplingRowScan
	}
	return applyCombinedSamplingDecision(decision, mode, result, frame)
}

func finishCombinedExplorerRow(query ExplorerQuery, mode combinedScanMode, acc *explorerAccumulator, result *ExplorerResult, rowID, effective uint64, deferred []int, control *ExplorerControl) bool {
	stopAfterMatched := updateCombinedRowCounters(mode, result, effective, control)
	applyCombinedDeferredValues(query, acc, result, effective, deferred)
	if query.Histogram != nil {
		acc.finishHistogramRow(rowID, effective, &result.Stats)
	}
	if mode.includeFacets {
		acc.finishFacetRow(rowID, &result.Stats)
	}
	return stopAfterMatched
}

func updateCombinedRowCounters(mode combinedScanMode, result *ExplorerResult, effective uint64, control *ExplorerControl) bool {
	stopAfterMatched := false
	if mode.includeMain {
		result.Stats.RowsMatched++
		stopAfterMatched = control != nil && control.emitMatchedRow(effective, result.Stats.RowsMatched)
	}
	if mode.includeFacets {
		result.Stats.FacetRowsMatched++
	}
	return stopAfterMatched
}

func applyCombinedDeferredValues(query ExplorerQuery, acc *explorerAccumulator, result *ExplorerResult, effective uint64, deferred []int) {
	var histogramRealtime *uint64
	if query.Histogram != nil {
		histogramRealtime = &effective
	}
	for _, valueIndex := range deferred {
		acc.applyValue(valueIndex, histogramRealtime, &result.Stats)
	}
}

func (r *Reader) scanExplorerFacet(query ExplorerQuery, candidates explorerCandidateSet, acc *explorerAccumulator, stats *ExplorerStats, control *ExplorerControl) error {
	r.seekForExplorer(query)
	candidates.prepare(query.Direction)
	needsFTS := queryHasFTS(query)
	deferApply := query.AfterRealtimeUsec != nil || query.BeforeRealtimeUsec != nil || needsFTS
	var rowID, rowsSeen uint64
	var deferred []int
	for {
		frame, ok, stop, err := r.nextExplorerRowFrame(query, &candidates, &rowsSeen, *stats, control)
		if err != nil || stop {
			return err
		}
		if !ok {
			break
		}
		rowID++
		deferred = deferred[:0]
		apply := scanApplyImmediate
		if deferApply {
			apply = scanApplyDeferred
		}
		scan, err := r.scanCurrentRow(query, acc, rowID, apply, &deferred, stats, needsFTS)
		if err != nil {
			return err
		}
		if _, accepted := acceptedEffectiveRealtime(query, scan, frame.commitRealtime, stats, nil); !accepted {
			continue
		}
		recordLastRealtime(stats, frame.commitRealtime)
		stats.FacetRowsMatched++
		if deferApply {
			for _, valueIndex := range deferred {
				acc.applyValue(valueIndex, nil, stats)
			}
		}
		acc.finishFacetRow(rowID, stats)
	}
	return nil
}

type explorerRowFrame struct {
	commitRealtime uint64
	seqnum         uint64
}

func (r *Reader) nextExplorerRowFrame(query ExplorerQuery, candidates *explorerCandidateSet, rowsSeen *uint64, stats ExplorerStats, control *ExplorerControl) (explorerRowFrame, bool, bool, error) {
	for {
		offset, ok, err := r.nextExplorerOffset(query.Direction, candidates)
		if err != nil || !ok {
			return explorerRowFrame{}, false, false, err
		}
		*rowsSeen++
		if control != nil && control.shouldStopAfterRows(*rowsSeen, stats) {
			return explorerRowFrame{}, false, true, nil
		}
		frame, keep, stop, err := r.explorerFrameForOffset(query, offset)
		if err != nil || stop {
			return explorerRowFrame{}, false, stop, err
		}
		if !keep {
			continue
		}
		return frame, true, false, nil
	}
}

func (r *Reader) nextExplorerOffset(direction Direction, candidates *explorerCandidateSet) (uint64, bool, error) {
	if candidates != nil && !candidates.all {
		return r.nextCandidateExplorerOffset(direction, candidates)
	}
	return r.nextSequentialExplorerOffset(direction, candidates)
}

func (r *Reader) nextCandidateExplorerOffset(direction Direction, candidates *explorerCandidateSet) (uint64, bool, error) {
	for {
		offset, ok := candidates.nextOffset(direction)
		if !ok {
			return 0, false, nil
		}
		if r.setCurrentEntryOffset(offset, direction) {
			return offset, true, nil
		}
	}
}

func (r *Reader) nextSequentialExplorerOffset(direction Direction, candidates *explorerCandidateSet) (uint64, bool, error) {
	for {
		if err := r.stepExplorerDirection(direction); err != nil {
			if errors.Is(err, errEndOfEntries) || errors.Is(err, errStartOfEntries) {
				return 0, false, nil
			}
			return 0, false, err
		}
		offset, err := r.currentEntryOffset()
		if err != nil {
			return 0, false, err
		}
		if candidates == nil || candidates.contains(offset) {
			return offset, true, nil
		}
	}
}

func (r *Reader) stepExplorerDirection(direction Direction) error {
	if direction == DirectionBackward {
		return r.Previous()
	}
	return r.Next()
}

func (r *Reader) explorerFrameForOffset(query ExplorerQuery, offset uint64) (explorerRowFrame, bool, bool, error) {
	header, err := r.readEntryHeaderAt(offset)
	if err != nil {
		return explorerRowFrame{}, false, false, err
	}
	if stopByCommitTime(query, header.realtime) {
		return explorerRowFrame{}, false, true, nil
	}
	if skipByCommitTime(query, header.realtime) {
		return explorerRowFrame{}, false, false, nil
	}
	return explorerRowFrame{commitRealtime: header.realtime, seqnum: header.seqnum}, true, false, nil
}

func (r *Reader) setCurrentEntryOffset(offset uint64, direction Direction) bool {
	index := sort.Search(len(r.entryOffsets), func(i int) bool {
		return r.entryOffsets[i] >= offset
	})
	if index >= len(r.entryOffsets) || r.entryOffsets[index] != offset {
		return false
	}
	r.clearCurrentEntryState()
	r.entryIndex = index
	r.direction = direction
	r.realtimeSeek = nil
	return true
}

func (r *Reader) scanRowDataOrDefault(query ExplorerQuery, acc *explorerAccumulator, rowID *uint64, deferred *[]int, stats *ExplorerStats, needsFTS bool) (rowScan, error) {
	if acc.requiredIdentityCount == 0 && !needsFTS {
		stats.RowsExamined++
		return rowScan{}, nil
	}
	*rowID++
	*deferred = (*deferred)[:0]
	return r.scanCurrentRow(query, acc, *rowID, scanApplyDeferred, deferred, stats, needsFTS)
}

type scanApplyMode int

const (
	scanApplyImmediate scanApplyMode = iota
	scanApplyDeferred
)

func (r *Reader) scanCurrentRow(query ExplorerQuery, acc *explorerAccumulator, rowID uint64, apply scanApplyMode, deferred *[]int, stats *ExplorerStats, needsFTS bool) (rowScan, error) {
	stats.RowsExamined++
	var out rowScan
	state := newRowScanState(query, acc, needsFTS)
	acc.rowPublicFieldIndices = acc.rowPublicFieldIndices[:0]
	offsets, err := r.currentEntryDataOffsets()
	if err != nil {
		return out, err
	}
	for _, dataOffset := range offsets {
		stats.DataRefsSeen++
		class, err := r.classifyDataForAccumulator(dataOffset, acc, needsFTS, query, stats)
		if err != nil {
			return out, err
		}
		handleRowOffsetClass(class, acc, rowID, &state, &out, apply, deferred, stats)
		if state.shouldStopRowScan() {
			stats.EarlyStopOpportunities++
			stats.EarlyStops++
			break
		}
	}
	return out, nil
}

type rowScanState struct {
	useFirstValue      bool
	needsFTS           bool
	fieldsMissingInRow int
}

func newRowScanState(query ExplorerQuery, acc *explorerAccumulator, needsFTS bool) rowScanState {
	useFirst := query.FieldMode == ExplorerFieldModeFirstValue
	missing := 0
	if useFirst {
		missing = acc.requiredIdentityCount
	}
	return rowScanState{useFirstValue: useFirst, needsFTS: needsFTS, fieldsMissingInRow: missing}
}

func (s rowScanState) shouldStopRowScan() bool {
	return s.useFirstValue && !s.needsFTS && s.fieldsMissingInRow == 0
}

func (r *Reader) classifyDataForAccumulator(dataOffset uint64, acc *explorerAccumulator, needsFTS bool, query ExplorerQuery, stats *ExplorerStats) (offsetClass, error) {
	if class, ok := acc.offsetCache.lookup(dataOffset); ok {
		stats.DataCacheHits++
		return class, nil
	}
	stats.DataCacheMisses++
	stats.DataPayloadsLoaded++
	header, err := r.readDataHeaderAt(dataOffset)
	if err != nil {
		return offsetClass{}, err
	}
	payload, err := r.readDataPayloadWithHeader(dataOffset, header)
	if err != nil {
		return offsetClass{}, err
	}
	if header.object.flag&(objectCompressedXZ|objectCompressedLZ4|objectCompressedZSTD) != 0 {
		stats.PayloadsDecompressed++
	}
	field, value, ok := splitRawPayload(payload)
	if !ok {
		class := classifyUnstructuredPayload(payload, needsFTS, query, stats)
		acc.offsetCache.insert(dataOffset, class)
		stats.DataObjectsClassified++
		return class, nil
	}
	ftsMatch, ftsNegative := ftsFlagsForValue(value, needsFTS, query, stats)
	class := structuredPayloadClass(field, value, acc, ftsMatch, ftsNegative)
	acc.offsetCache.insert(dataOffset, class)
	stats.DataObjectsClassified++
	return class, nil
}

func structuredPayloadClass(field, value []byte, acc *explorerAccumulator, ftsMatch, ftsNegative bool) offsetClass {
	if ftsNegative {
		return offsetClass{kind: offsetClassFtsNegative}
	}
	if fieldIndex, ok := acc.fieldLookup[string(field)]; ok {
		return offsetClass{kind: offsetClassValue, valueIndex: acc.addValue(fieldIndex, value, ftsMatch)}
	}
	if ftsMatch {
		return offsetClass{kind: offsetClassFtsMatch}
	}
	return offsetClass{kind: offsetClassIrrelevant}
}

func classifyUnstructuredPayload(payload []byte, needsFTS bool, query ExplorerQuery, stats *ExplorerStats) offsetClass {
	if !needsFTS {
		return offsetClass{kind: offsetClassIrrelevant}
	}
	stats.FTSScans++
	switch matchFTSQuery(payload, query) {
	case ftsTermPositive:
		return offsetClass{kind: offsetClassFtsMatch}
	case ftsTermNegative:
		return offsetClass{kind: offsetClassFtsNegative}
	default:
		return offsetClass{kind: offsetClassIrrelevant}
	}
}

func handleRowOffsetClass(class offsetClass, acc *explorerAccumulator, rowID uint64, state *rowScanState, out *rowScan, apply scanApplyMode, deferred *[]int, stats *ExplorerStats) {
	switch class.kind {
	case offsetClassIrrelevant:
		stats.DataRefsSkipped++
	case offsetClassFtsNegative:
		out.ftsNegativeMatch = true
	case offsetClassFtsMatch:
		out.ftsMatches = true
	case offsetClassValue:
		handleRowValueClass(class.valueIndex, acc, rowID, state, out, apply, deferred, stats)
	}
}

func handleRowValueClass(valueIndex int, acc *explorerAccumulator, rowID uint64, state *rowScanState, out *rowScan, apply scanApplyMode, deferred *[]int, stats *ExplorerStats) {
	if acc.valueFTSMatches[valueIndex] {
		out.ftsMatches = true
	}
	fieldIndex := acc.valueFieldIndices[valueIndex]
	firstForField := true
	if state.useFirstValue || acc.flags[fieldIndex]&(facetPublic|facetHistogram) != 0 {
		firstForField = acc.markFieldSeen(fieldIndex, rowID)
	}
	if state.useFirstValue && firstForField {
		state.fieldsMissingInRow--
	}
	if firstForField && acc.flags[fieldIndex]&facetPublic != 0 {
		acc.rowPublicFieldIndices = append(acc.rowPublicFieldIndices, fieldIndex)
	}
	if state.useFirstValue && !firstForField {
		return
	}
	if timestamp := acc.valueSourceRealtime[valueIndex]; timestamp != nil {
		out.timestamp = timestamp
	}
	if apply == scanApplyImmediate {
		acc.applyValue(valueIndex, nil, stats)
	} else {
		*deferred = append(*deferred, valueIndex)
	}
}

func acceptedEffectiveRealtime(query ExplorerQuery, scan rowScan, commitRealtime uint64, stats *ExplorerStats, control *ExplorerControl) (uint64, bool) {
	effective := effectiveRealtimeFromScan(scan.timestamp, commitRealtime)
	recordSourceRealtimeDelta(stats, scan.timestamp, commitRealtime)
	_ = control
	return effective, timestampInRange(query, effective) && !rowRejectedByFTS(query, scan)
}

func (r *Reader) pushExplorerRowIfWanted(query ExplorerQuery, result *ExplorerResult, rowPayloadMode explorerRowPayloadMode, effectiveRealtime uint64) error {
	if !rowWithinAnchor(query, effectiveRealtime) || len(result.Rows) >= query.Limit {
		return nil
	}
	row, err := r.currentExplorerRow(effectiveRealtime, &result.Stats, rowPayloadMode)
	if err != nil {
		return err
	}
	result.Rows = append(result.Rows, row)
	return nil
}

func (r *Reader) currentExplorerRow(realtimeUsec uint64, stats *ExplorerStats, rowPayloadMode explorerRowPayloadMode) (ExplorerRow, error) {
	cursor, err := r.GetCursor()
	if err != nil {
		return ExplorerRow{}, err
	}
	var payloads [][]byte
	if rowPayloadMode == explorerRowPayloadExpand {
		payloads, err = r.CollectEntryPayloads()
		if err != nil {
			return ExplorerRow{}, err
		}
		stats.ReturnedRowExpansions++
	}
	return ExplorerRow{RealtimeUsec: realtimeUsec, Cursor: cursor, Payloads: payloads}, nil
}

func (r *Reader) seekForExplorer(query ExplorerQuery) {
	anchor := query.Anchor
	if !query.StopWhenRowsFull {
		anchor = DefaultExplorerAnchor()
	}
	if query.Direction == DirectionBackward {
		r.seekForExplorerBackward(query, anchor)
		return
	}
	r.seekForExplorerForward(query, anchor)
}

func (r *Reader) seekForExplorerBackward(query ExplorerQuery, anchor ExplorerAnchor) {
	switch anchor.Kind {
	case ExplorerAnchorRealtime:
		r.seekExplorerBackwardRealtime(anchor.RealtimeUsec)
	case ExplorerAnchorHead:
		_ = r.SeekHead()
	case ExplorerAnchorTail:
		r.seekExplorerBackwardTail(query)
	default:
		r.seekExplorerBackwardTail(query)
	}
}

func (r *Reader) seekExplorerBackwardTail(query ExplorerQuery) {
	if query.BeforeRealtimeUsec != nil {
		r.seekExplorerBackwardRealtime(saturatingAdd(*query.BeforeRealtimeUsec, query.RealtimeSlackUsec))
		return
	}
	_ = r.SeekTail()
}

func (r *Reader) seekForExplorerForward(query ExplorerQuery, anchor ExplorerAnchor) {
	switch anchor.Kind {
	case ExplorerAnchorRealtime:
		_ = r.SeekRealtimeUsec(anchor.RealtimeUsec)
	case ExplorerAnchorTail:
		_ = r.SeekTail()
	case ExplorerAnchorHead:
		r.seekExplorerForwardHead(query)
	default:
		r.seekExplorerForwardHead(query)
	}
}

func (r *Reader) seekExplorerForwardHead(query ExplorerQuery) {
	if query.AfterRealtimeUsec != nil {
		_ = r.SeekRealtimeUsec(saturatingSub(*query.AfterRealtimeUsec, query.RealtimeSlackUsec))
		return
	}
	_ = r.SeekHead()
}

func (r *Reader) seekExplorerBackwardRealtime(usec uint64) {
	r.clearCurrentEntryState()
	idx, err := r.firstRealtimeIndexAtOrAfter(usec)
	if err != nil {
		r.entryIndex = -1
		return
	}
	if idx >= len(r.entryOffsets) {
		r.entryIndex = len(r.entryOffsets)
	} else {
		r.entryIndex = idx + 1
	}
	r.direction = DirectionBackward
	r.realtimeSeek = nil
}

type combinedScanMode struct {
	includeMain   bool
	includeFacets bool
}

type samplingRowAction int

const (
	samplingRowScan samplingRowAction = iota
	samplingRowSkip
	samplingRowStop
)

func samplingStateForCombined(query ExplorerQuery, result *ExplorerResult, control *ExplorerControl) *explorerSamplingState {
	var bucketCount int
	if result.Histogram != nil {
		bucketCount = len(result.Histogram.Buckets)
	}
	if control != nil && control.sampling != nil {
		if query.Sampling != nil {
			control.sampling.beginFile(*query.Sampling)
		}
		return control.sampling
	}
	return newExplorerSamplingState(query, bucketCount)
}

func combinedSamplingDecision(query ExplorerQuery, rows []ExplorerRow, frame explorerRowFrame, sampling *explorerSamplingState, control *ExplorerControl) (explorerSamplingDecision, bool) {
	if sampling == nil {
		return explorerSamplingDecision{}, false
	}
	candidateToKeep := rowCandidateToKeep(query, rows, frame.commitRealtime)
	if control != nil && control.candidateRow != nil {
		candidateToKeep = control.candidateRow(frame.commitRealtime)
	}
	return sampling.decide(frame.commitRealtime, frame.seqnum, candidateToKeep), true
}

func applyCombinedSamplingDecision(decision explorerSamplingDecision, mode combinedScanMode, result *ExplorerResult, frame explorerRowFrame) samplingRowAction {
	switch decision.kind {
	case explorerSamplingFull:
		if decision.sampled {
			result.Stats.SamplingSampled++
		}
		return samplingRowScan
	case explorerSamplingSkipFields:
		recordCombinedUnsampledRow(&result.Stats, mode, frame.commitRealtime, 1, true)
		addSpecialHistogramValue(result.Histogram, frame.commitRealtime, explorerUnsampledValue, 1, &result.Stats)
		return samplingRowSkip
	case explorerSamplingStopAndEstimate:
		recordCombinedUnsampledRow(&result.Stats, mode, frame.commitRealtime, decision.remainingRows, false)
		result.Stats.RowsEstimated += decision.remainingRows
		result.Stats.SamplingEstimated += decision.remainingRows
		addEstimatedHistogramRange(result.Histogram, decision.fromRealtimeUsec, decision.toRealtimeUsec, decision.remainingRows, &result.Stats)
		return samplingRowStop
	default:
		return samplingRowScan
	}
}

func recordCombinedUnsampledRow(stats *ExplorerStats, mode combinedScanMode, commitRealtime, rowCount uint64, countRowsUnsampled bool) {
	recordLastRealtime(stats, commitRealtime)
	if mode.includeMain {
		stats.RowsMatched += rowCount
	}
	if mode.includeFacets {
		stats.FacetRowsMatched += rowCount
	}
	if countRowsUnsampled {
		stats.RowsUnsampled += rowCount
	}
	stats.SamplingUnsampled++
}

func addSpecialHistogramValue(histogram *ExplorerHistogram, realtimeUsec uint64, value []byte, count uint64, stats *ExplorerStats) {
	if histogram == nil {
		return
	}
	bucketIndex := histogramBucketIndex(histogram, realtimeUsec)
	if bucketIndex < 0 || bucketIndex >= len(histogram.Buckets) {
		return
	}
	histogram.Buckets[bucketIndex].Values[string(value)] += count
	stats.HistogramUpdates++
}

func addEstimatedHistogramRange(histogram *ExplorerHistogram, fromRealtimeUsec, toRealtimeUsec, entries uint64, stats *ExplorerStats) {
	if histogram == nil || entries == 0 || fromRealtimeUsec >= toRealtimeUsec || len(histogram.Buckets) == 0 {
		return
	}
	first := histogram.Buckets[0]
	last := histogram.Buckets[len(histogram.Buckets)-1]
	fromRealtimeUsec = maxU64(fromRealtimeUsec, first.StartRealtimeUsec)
	toRealtimeUsec = minUint64(toRealtimeUsec, last.EndRealtimeUsec)
	if fromRealtimeUsec >= toRealtimeUsec {
		return
	}
	total := maxU64(deltaValue(toRealtimeUsec, fromRealtimeUsec), 1)
	var touched uint64
	for index := range histogram.Buckets {
		bucket := &histogram.Buckets[index]
		if bucket.StartRealtimeUsec > toRealtimeUsec {
			break
		}
		if bucket.EndRealtimeUsec <= fromRealtimeUsec {
			continue
		}
		overlapStart := maxU64(bucket.StartRealtimeUsec, fromRealtimeUsec)
		overlapEnd := minUint64(bucket.EndRealtimeUsec, toRealtimeUsec)
		if overlapStart >= overlapEnd {
			continue
		}
		share := mulDivU64(deltaValue(overlapEnd, overlapStart), entries, total)
		if share != 0 {
			bucket.Values[string(explorerEstimatedValue)] += share
		}
		stats.HistogramUpdates++
		touched++
	}
}

func mulDivU64(left, right, divisor uint64) uint64 {
	if divisor == 0 {
		return 0
	}
	hi, lo := bits.Mul64(left, right)
	if hi >= divisor {
		return ^uint64(0)
	}
	quotient, _ := bits.Div64(hi, lo, divisor)
	return quotient
}

type facetPassGroup struct {
	excludedField []byte
	facetIndices  []int
}

func facetPassGroups(query ExplorerQuery) []facetPassGroup {
	filterFields := make(map[string]struct{})
	for _, filter := range query.Filters {
		filterFields[string(filter.Field)] = struct{}{}
	}
	var groups []facetPassGroup
	for index, facet := range query.Facets {
		var excluded []byte
		if query.ExcludeFacetFieldFilters {
			if _, ok := filterFields[string(facet)]; ok {
				excluded = facet
			}
		}
		found := false
		for groupIndex := range groups {
			if bytes.Equal(groups[groupIndex].excludedField, excluded) {
				groups[groupIndex].facetIndices = append(groups[groupIndex].facetIndices, index)
				found = true
				break
			}
		}
		if !found {
			groups = append(groups, facetPassGroup{excludedField: cloneBytes(excluded), facetIndices: []int{index}})
		}
	}
	return groups
}

func canRunCombinedExplorerPass(groups []facetPassGroup) bool {
	for _, group := range groups {
		if group.excludedField != nil {
			return false
		}
	}
	return true
}

func combinedFacetIndices(groups []facetPassGroup) []int {
	var out []int
	for _, group := range groups {
		out = append(out, group.facetIndices...)
	}
	return out
}

func (r *Reader) explorerResultForQuery(query ExplorerQuery) (ExplorerResult, error) {
	result := ExplorerResult{
		Facets:       make(map[string]map[string]uint64),
		ColumnFields: make(map[string]struct{}),
	}
	fields, err := r.EnumerateFields()
	if err != nil {
		return result, err
	}
	for field := range fields {
		result.ColumnFields[field] = struct{}{}
	}
	if query.Histogram != nil {
		histogram := newExplorerHistogram(query.Histogram, query)
		result.Histogram = &histogram
	}
	return result, nil
}

func validateExplorerQuery(query ExplorerQuery) error {
	if query.AfterRealtimeUsec != nil && query.BeforeRealtimeUsec != nil && *query.AfterRealtimeUsec > *query.BeforeRealtimeUsec {
		return fmt.Errorf("%w: after_realtime_usec must be <= before_realtime_usec", errInvalidJournal)
	}
	for _, filter := range query.Filters {
		if invalidExplorerField(filter.Field) {
			return fmt.Errorf("%w: filter field must be non-empty and must not contain '='", errInvalidJournal)
		}
	}
	for _, field := range query.Facets {
		if invalidExplorerField(field) {
			return fmt.Errorf("%w: facet fields must be non-empty and must not contain '='", errInvalidJournal)
		}
	}
	if query.Histogram != nil && invalidExplorerField(query.Histogram) {
		return fmt.Errorf("%w: histogram field must be non-empty and must not contain '='", errInvalidJournal)
	}
	seen := make(map[string]struct{})
	for _, facet := range query.Facets {
		key := string(facet)
		if _, ok := seen[key]; ok {
			return fmt.Errorf("%w: facet fields must not be duplicated", errInvalidJournal)
		}
		seen[key] = struct{}{}
	}
	return nil
}

func validateIndexedExplorerQuery(query ExplorerQuery) error {
	if query.FieldMode != ExplorerFieldModeAllValues {
		return fmt.Errorf("%w: indexed explorer strategy requires ExplorerFieldModeAllValues", ErrUnsupported)
	}
	if queryHasFTS(query) {
		return fmt.Errorf("%w: indexed explorer strategy does not support FTS", ErrUnsupported)
	}
	if query.UseSourceRealtime && (query.AfterRealtimeUsec != nil || query.BeforeRealtimeUsec != nil || query.Histogram != nil) {
		return fmt.Errorf("%w: indexed explorer strategy requires commit realtime for time-bounded facets and histograms", ErrUnsupported)
	}
	return nil
}

func invalidExplorerField(field []byte) bool {
	return len(field) == 0 || bytes.IndexByte(field, '=') >= 0
}

func queryNeedsMainPass(query ExplorerQuery) bool {
	return query.Limit > 0 || query.Histogram != nil
}

func queryNeedsSourceRealtimeMain(query ExplorerQuery) bool {
	return query.UseSourceRealtime && (query.AfterRealtimeUsec != nil || query.BeforeRealtimeUsec != nil || query.Histogram != nil || query.Limit > 0)
}

func facetPassNeedsSourceRealtime(query ExplorerQuery) bool {
	return query.UseSourceRealtime && (query.AfterRealtimeUsec != nil || query.BeforeRealtimeUsec != nil)
}

func queryHasFTS(query ExplorerQuery) bool {
	return len(query.FTSTerms) != 0 || len(query.FTSPatterns) != 0 || len(query.FTSNegative) != 0
}

func queryHasPositiveFTS(query ExplorerQuery) bool {
	if len(query.FTSTerms) != 0 {
		for _, term := range query.FTSTerms {
			if !term.Negative {
				return true
			}
		}
		return false
	}
	return len(query.FTSPatterns) != 0
}

func rowRejectedByFTS(query ExplorerQuery, scan rowScan) bool {
	return queryHasFTS(query) && (scan.ftsNegativeMatch || queryHasPositiveFTS(query) && !scan.ftsMatches)
}

type ftsTermMatch int

const (
	ftsTermNone ftsTermMatch = iota
	ftsTermPositive
	ftsTermNegative
)

func matchFTSQuery(value []byte, query ExplorerQuery) ftsTermMatch {
	if len(query.FTSTerms) != 0 {
		for _, term := range query.FTSTerms {
			if term.matches(value) {
				if term.Negative {
					return ftsTermNegative
				}
				return ftsTermPositive
			}
		}
		return ftsTermNone
	}
	if matchesFTS(value, query.FTSNegative) {
		return ftsTermNegative
	}
	if matchesFTS(value, query.FTSPatterns) {
		return ftsTermPositive
	}
	return ftsTermNone
}

func matchesFTS(value []byte, patterns [][]byte) bool {
	for _, pattern := range patterns {
		if len(pattern) != 0 && containsASCIIInsensitive(value, pattern) {
			return true
		}
	}
	return false
}

func ftsFlagsForValue(value []byte, needsFTS bool, query ExplorerQuery, stats *ExplorerStats) (bool, bool) {
	if !needsFTS {
		return false, false
	}
	stats.FTSScans++
	switch matchFTSQuery(value, query) {
	case ftsTermPositive:
		return true, false
	case ftsTermNegative:
		return false, true
	default:
		return false, false
	}
}

func containsASCIIInsensitive(haystack, needle []byte) bool {
	return findASCIIInsensitive(haystack, needle) >= 0
}

func findASCIIInsensitive(haystack, needle []byte) int {
	if len(needle) == 0 {
		return 0
	}
	if len(haystack) < len(needle) {
		return -1
	}
	for i := 0; i <= len(haystack)-len(needle); i++ {
		if asciiEqualFold(haystack[i:i+len(needle)], needle) {
			return i
		}
	}
	return -1
}

func asciiEqualFold(left, right []byte) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		a, b := left[i], right[i]
		if 'A' <= a && a <= 'Z' {
			a += 'a' - 'A'
		}
		if 'A' <= b && b <= 'Z' {
			b += 'a' - 'A'
		}
		if a != b {
			return false
		}
	}
	return true
}

func stopByCommitTime(query ExplorerQuery, commitRealtime uint64) bool {
	switch query.Direction {
	case DirectionBackward:
		return query.AfterRealtimeUsec != nil && commitRealtime < *query.AfterRealtimeUsec
	default:
		return query.BeforeRealtimeUsec != nil && commitRealtime > saturatingAdd(*query.BeforeRealtimeUsec, query.RealtimeSlackUsec)
	}
}

func skipByCommitTime(query ExplorerQuery, commitRealtime uint64) bool {
	switch query.Direction {
	case DirectionBackward:
		return query.BeforeRealtimeUsec != nil && commitRealtime > saturatingAdd(*query.BeforeRealtimeUsec, query.RealtimeSlackUsec)
	default:
		return query.AfterRealtimeUsec != nil && commitRealtime < *query.AfterRealtimeUsec
	}
}

func timestampInRange(query ExplorerQuery, timestamp uint64) bool {
	if query.AfterRealtimeUsec != nil && timestamp < *query.AfterRealtimeUsec {
		return false
	}
	if query.BeforeRealtimeUsec != nil && timestamp > *query.BeforeRealtimeUsec {
		return false
	}
	return true
}

func rowWithinAnchor(query ExplorerQuery, realtimeUsec uint64) bool {
	if query.Anchor.Kind != ExplorerAnchorRealtime {
		return true
	}
	if query.Direction == DirectionForward {
		return realtimeUsec > query.Anchor.RealtimeUsec
	}
	return realtimeUsec <= query.Anchor.RealtimeUsec
}

func rowCandidateToKeep(query ExplorerQuery, rows []ExplorerRow, realtimeUsec uint64) bool {
	if query.Limit == 0 || !rowWithinAnchor(query, realtimeUsec) {
		return false
	}
	if len(rows) < query.Limit {
		return true
	}
	if query.Direction == DirectionBackward {
		oldest := rows[0].RealtimeUsec
		for _, row := range rows[1:] {
			if row.RealtimeUsec < oldest {
				oldest = row.RealtimeUsec
			}
		}
		return realtimeUsec >= oldest
	}
	newest := rows[0].RealtimeUsec
	for _, row := range rows[1:] {
		if row.RealtimeUsec > newest {
			newest = row.RealtimeUsec
		}
	}
	return realtimeUsec <= newest
}

func shouldStopWhenRowsFull(query ExplorerQuery, rows []ExplorerRow, effectiveRealtime, rowsMatched uint64) bool {
	if !query.StopWhenRowsFull || query.Limit == 0 || len(rows) < query.Limit {
		return false
	}
	every := query.StopWhenRowsFullEvery
	if every == 0 {
		every = 1
	}
	if rowsMatched == 0 || rowsMatched%every != 0 {
		return false
	}
	if query.Direction == DirectionBackward {
		oldest := rows[0].RealtimeUsec
		for _, row := range rows[1:] {
			if row.RealtimeUsec < oldest {
				oldest = row.RealtimeUsec
			}
		}
		return effectiveRealtime < saturatingSub(oldest, query.RealtimeSlackUsec)
	}
	newest := rows[0].RealtimeUsec
	for _, row := range rows[1:] {
		if row.RealtimeUsec > newest {
			newest = row.RealtimeUsec
		}
	}
	return effectiveRealtime > saturatingAdd(newest, query.RealtimeSlackUsec)
}

func newExplorerHistogram(field []byte, query ExplorerQuery) ExplorerHistogram {
	start, end := histogramBounds(query)
	target := query.HistogramBuckets
	if target < 1 {
		target = 1
	}
	width := histogramBarWidthUsec(start, end, target)
	start = histogramSlotBaselineUsec(start, width)
	end = histogramSlotBaselineUsec(end, width) + width
	count := int((end-start)/width) + 1
	if count > 1001 {
		count = 1001
		width = (end - start) / 1000
		if width == 0 {
			width = 1
		}
		end = start + width*1000
	}
	buckets := make([]ExplorerHistogramBucket, 0, count)
	for i := 0; i < count; i++ {
		bucketStart := start + width*uint64(i)
		bucketEnd := bucketStart + width
		if i+1 == count {
			bucketEnd = end + 1
		}
		buckets = append(buckets, ExplorerHistogramBucket{
			StartRealtimeUsec: bucketStart,
			EndRealtimeUsec:   bucketEnd,
			Values:            make(map[string]uint64),
		})
	}
	return ExplorerHistogram{Field: cloneBytes(field), Buckets: buckets}
}

func histogramBarWidthUsec(after, before uint64, targetBuckets int) uint64 {
	validSeconds := []uint64{1, 2, 5, 10, 15, 30, 60, 120, 180, 300, 600, 900, 1800, 3600, 7200, 21600, 28800, 43200, 86400, 172800, 259200, 432000, 604800, 1209600, 2592000}
	duration := before - after
	for i := len(validSeconds) - 1; i >= 0; i-- {
		width := validSeconds[i] * 1_000_000
		if width != 0 && duration/width >= uint64(targetBuckets) {
			return width
		}
	}
	return 1_000_000
}

func histogramSlotBaselineUsec(value, width uint64) uint64 {
	if width == 0 {
		width = 1
	}
	return value - value%width
}

func histogramBounds(query ExplorerQuery) (uint64, uint64) {
	start := uint64(0)
	if query.HistogramAfterUsec != nil {
		start = *query.HistogramAfterUsec
	} else if query.AfterRealtimeUsec != nil {
		start = *query.AfterRealtimeUsec
	}
	end := start + 3_600_000_000
	if query.HistogramBeforeUsec != nil {
		end = *query.HistogramBeforeUsec
	} else if query.BeforeRealtimeUsec != nil {
		end = *query.BeforeRealtimeUsec
	}
	if end <= start {
		return start, start + 1
	}
	return start, end
}

func histogramBucketIndexFromBounds(realtimeUsec, start, width uint64, count int) int {
	if count == 0 {
		return -1
	}
	if width == 0 {
		width = 1
	}
	if realtimeUsec < start {
		return 0
	}
	index := int((realtimeUsec - start) / width)
	if index >= count {
		index = count - 1
	}
	return index
}

func histogramBucketIndex(histogram *ExplorerHistogram, realtimeUsec uint64) int {
	if histogram == nil || len(histogram.Buckets) == 0 {
		return -1
	}
	first := histogram.Buckets[0]
	width := first.EndRealtimeUsec - first.StartRealtimeUsec
	if width == 0 {
		width = 1
	}
	return histogramBucketIndexFromBounds(realtimeUsec, first.StartRealtimeUsec, width, len(histogram.Buckets))
}

func parseSourceRealtime(value []byte) *uint64 {
	parsed, err := strconv.ParseUint(string(value), 10, 64)
	if err != nil {
		return nil
	}
	return &parsed
}

func effectiveRealtimeFromScan(sourceRealtime *uint64, commitRealtime uint64) uint64 {
	if sourceRealtime != nil && *sourceRealtime != 0 && *sourceRealtime < commitRealtime {
		return *sourceRealtime
	}
	return commitRealtime
}

func recordLastRealtime(stats *ExplorerStats, commitRealtime uint64) {
	if commitRealtime > stats.LastRealtimeUsec {
		stats.LastRealtimeUsec = commitRealtime
	}
}

func recordSourceRealtimeDelta(stats *ExplorerStats, sourceRealtime *uint64, commitRealtime uint64) {
	if sourceRealtime == nil || *sourceRealtime == 0 || *sourceRealtime >= commitRealtime {
		return
	}
	delta := commitRealtime - *sourceRealtime
	if delta > stats.MaxSourceRealtimeDeltaUsec {
		stats.MaxSourceRealtimeDeltaUsec = delta
	}
}

func payloadFromParts(field, value []byte) []byte {
	out := make([]byte, 0, len(field)+1+len(value))
	out = append(out, field...)
	out = append(out, '=')
	out = append(out, value...)
	return out
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func maxU64(a, b uint64) uint64 {
	if a > b {
		return a
	}
	return b
}

func deltaValue(value, base uint64) uint64 {
	if value < base {
		return 0
	}
	return value - base
}

func explorerOutputsMatch(left, right ExplorerResult) bool {
	if len(left.Rows) != len(right.Rows) || len(left.Facets) != len(right.Facets) {
		return false
	}
	for i := range left.Rows {
		if left.Rows[i].RealtimeUsec != right.Rows[i].RealtimeUsec || left.Rows[i].Cursor != right.Rows[i].Cursor || !payloadListsEqual(left.Rows[i].Payloads, right.Rows[i].Payloads) {
			return false
		}
	}
	if !facetMapsEqual(left.Facets, right.Facets) {
		return false
	}
	return histogramsEqual(left.Histogram, right.Histogram)
}

func payloadListsEqual(left, right [][]byte) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if !bytes.Equal(left[i], right[i]) {
			return false
		}
	}
	return true
}

func facetMapsEqual(left, right map[string]map[string]uint64) bool {
	if len(left) != len(right) {
		return false
	}
	for field, leftValues := range left {
		rightValues, ok := right[field]
		if !ok || len(leftValues) != len(rightValues) {
			return false
		}
		for value, count := range leftValues {
			if rightValues[value] != count {
				return false
			}
		}
	}
	return true
}

func histogramsEqual(left, right *ExplorerHistogram) bool {
	if left == nil || right == nil {
		return left == right
	}
	if !bytes.Equal(left.Field, right.Field) || len(left.Buckets) != len(right.Buckets) {
		return false
	}
	for i := range left.Buckets {
		lb, rb := left.Buckets[i], right.Buckets[i]
		if lb.StartRealtimeUsec != rb.StartRealtimeUsec || lb.EndRealtimeUsec != rb.EndRealtimeUsec || len(lb.Values) != len(rb.Values) {
			return false
		}
		for value, count := range lb.Values {
			if rb.Values[value] != count {
				return false
			}
		}
	}
	return true
}
