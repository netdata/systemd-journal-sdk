package journal

import (
	"bytes"
	"container/heap"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
)

const (
	defaultNetdataFunctionName                = "systemd-journal"
	defaultNetdataItemsToReturn               = 200
	defaultNetdataTimeWindowSeconds           = int64(3600)
	defaultNetdataItemsSampling               = uint64(1_000_000)
	netdataDataOnlyCheckEveryRows             = uint64(128)
	netdataRelativeTimeMaxSeconds             = int64(3 * 365 * 86_400)
	netdataMissingAfterRelativeSeconds        = int64(600)
	netdataDefaultHistogramBuckets            = 150
	netdataDisabledTimeoutSeconds             = uint64(100 * 365 * 24 * 60 * 60)
	netdataJournalRealtimeDeltaDefault        = uint64(5_000_000)
	netdataJournalRealtimeDeltaMax            = uint64(2 * 60 * 1_000_000)
	netdataEmptyStringFacetHashID             = "CzGfAU2z3TC"
	netdataUnavailableFieldLabel              = "[unavailable field]"
	netdataFacetMaxValueLength                = 8192
	netdataMaxDirectoryScanDepth              = 64
	netdataMaxDirectoryScanCount              = 8192
	netdataSourceTypeAll               uint64 = 1 << 0
	netdataSourceTypeLocalAll          uint64 = 1 << 1
	netdataSourceTypeRemoteAll         uint64 = 1 << 2
	netdataSourceTypeLocalSystem       uint64 = 1 << 3
	netdataSourceTypeLocalUser         uint64 = 1 << 4
	netdataSourceTypeLocalNamespace    uint64 = 1 << 5
	netdataSourceTypeLocalOther        uint64 = 1 << 6
)

const (
	NetdataSourceTypeAll            = netdataSourceTypeAll
	NetdataSourceTypeLocalAll       = netdataSourceTypeLocalAll
	NetdataSourceTypeRemoteAll      = netdataSourceTypeRemoteAll
	NetdataSourceTypeLocalSystem    = netdataSourceTypeLocalSystem
	NetdataSourceTypeLocalUser      = netdataSourceTypeLocalUser
	NetdataSourceTypeLocalNamespace = netdataSourceTypeLocalNamespace
	NetdataSourceTypeLocalOther     = netdataSourceTypeLocalOther
)

var netdataAcceptedParams = []string{
	"info",
	"__logs_sources",
	"after",
	"before",
	"anchor",
	"direction",
	"last",
	"query",
	"facets",
	"histogram",
	"if_modified_since",
	"data_only",
	"delta",
	"tail",
	"sampling",
	"slice",
}

var systemdDefaultViewKeys = []string{
	"_HOSTNAME",
	"ND_JOURNAL_PROCESS",
	"MESSAGE",
	"PRIORITY",
	"SYSLOG_FACILITY",
	"ERRNO",
	"ND_JOURNAL_FILE",
	"SYSLOG_IDENTIFIER",
	"UNIT",
	"USER_UNIT",
	"MESSAGE_ID",
	"_BOOT_ID",
	"_SYSTEMD_OWNER_UID",
	"_UID",
	"OBJECT_SYSTEMD_OWNER_UID",
	"OBJECT_UID",
	"_GID",
	"OBJECT_GID",
	"_CAP_EFFECTIVE",
	"_AUDIT_LOGINUID",
	"OBJECT_AUDIT_LOGINUID",
	"_SOURCE_REALTIME_TIMESTAMP",
}

var systemdDefaultFacets = []string{
	"_HOSTNAME",
	"PRIORITY",
	"SYSLOG_FACILITY",
	"ERRNO",
	"SYSLOG_IDENTIFIER",
	"UNIT",
	"USER_UNIT",
	"MESSAGE_ID",
	"_BOOT_ID",
	"_SYSTEMD_OWNER_UID",
	"_UID",
	"OBJECT_SYSTEMD_OWNER_UID",
	"OBJECT_UID",
	"_GID",
	"OBJECT_GID",
	"_AUDIT_LOGINUID",
	"OBJECT_AUDIT_LOGINUID",
	"CODE_FILE",
	"_SYSTEMD_UNIT",
	"_SYSTEMD_USER_SLICE",
	"CODE_FUNC",
	"_TRANSPORT",
	"_COMM",
	"_RUNTIME_SCOPE",
	"_MACHINE_ID",
	"_SYSTEMD_SLICE",
	"UNIT_RESULT",
	"_SYSTEMD_CGROUP",
	"_EXE",
	"_SYSTEMD_USER_UNIT",
	"_SYSTEMD_SESSION",
	"COREDUMP_CGROUP",
	"COREDUMP_USER_UNIT",
	"COREDUMP_UNIT",
	"COREDUMP_SIGNAL_NAME",
	"COREDUMP_COMM",
	"_UDEV_DEVNODE",
	"_KERNEL_SUBSYSTEM",
	"OBJECT_EXE",
	"OBJECT_SYSTEMD_CGROUP",
	"OBJECT_COMM",
	"OBJECT_SYSTEMD_UNIT",
	"OBJECT_SYSTEMD_USER_UNIT",
	"_SELINUX_CONTEXT",
	"_NAMESPACE",
	"OBJECT_SYSTEMD_SESSION",
	"CONTAINER_ID",
	"CONTAINER_NAME",
	"CONTAINER_TAG",
	"IMAGE_NAME",
	"ND_NIDL_NODE",
	"ND_NIDL_CONTEXT",
	"ND_LOG_SOURCE",
	"ND_ALERT_NAME",
	"ND_ALERT_CLASS",
	"ND_ALERT_COMPONENT",
	"ND_ALERT_TYPE",
	"ND_ALERT_STATUS",
}

type NetdataFunctionConfig struct {
	FunctionName     string
	DefaultFacets    []string
	DefaultViewKeys  []string
	DefaultHistogram string
	ReaderOptions    ReaderOptions
	ExplorerStrategy ExplorerStrategy
}

func SystemdJournalNetdataFunctionConfig() NetdataFunctionConfig {
	return NetdataFunctionConfig{
		FunctionName:     defaultNetdataFunctionName,
		DefaultFacets:    append([]string(nil), systemdDefaultFacets...),
		DefaultViewKeys:  append([]string(nil), systemdDefaultViewKeys...),
		DefaultHistogram: "PRIORITY",
		ReaderOptions:    DefaultReaderOptions().WithSnapshot(true),
		ExplorerStrategy: ExplorerStrategyTraversal,
	}
}

type DisplayScope int

const (
	DisplayScopeData DisplayScope = iota
	DisplayScopeFacet
	DisplayScopeHistogram
)

type DisplayContext struct {
	BootFirstRealtime map[string]uint64
	uidDisplayCache   map[string]string
	gidDisplayCache   map[string]string
}

func newDisplayContext() *DisplayContext {
	return &DisplayContext{
		BootFirstRealtime: make(map[string]uint64),
		uidDisplayCache:   make(map[string]string),
		gidDisplayCache:   make(map[string]string),
	}
}

type NetdataFunctionProfile interface {
	FieldDisplayValue(context *DisplayContext, scope DisplayScope, field string, value []byte) any
	FacetOptionName(context *DisplayContext, field string, value []byte) string
	RowOptions(fields map[string][][]byte) any
}

type SystemdJournalProfile struct{}
type SystemdJournalPluginProfile struct{}

func (SystemdJournalProfile) FieldDisplayValue(context *DisplayContext, scope DisplayScope, field string, value []byte) any {
	return systemdFieldDisplayValue(context, scope, field, value, false)
}

func (SystemdJournalPluginProfile) FieldDisplayValue(context *DisplayContext, scope DisplayScope, field string, value []byte) any {
	return systemdFieldDisplayValue(context, scope, field, value, true)
}

func (p SystemdJournalProfile) FacetOptionName(context *DisplayContext, field string, value []byte) string {
	return displayValueString(p.FieldDisplayValue(context, DisplayScopeFacet, field, value))
}

func (p SystemdJournalPluginProfile) FacetOptionName(context *DisplayContext, field string, value []byte) string {
	return displayValueString(p.FieldDisplayValue(context, DisplayScopeFacet, field, value))
}

func (SystemdJournalProfile) RowOptions(fields map[string][][]byte) any {
	return netdataRowOptions(fields)
}

func (SystemdJournalPluginProfile) RowOptions(fields map[string][][]byte) any {
	return netdataRowOptions(fields)
}

type NetdataJournalFunction struct {
	Config  NetdataFunctionConfig
	Profile NetdataFunctionProfile
}

func NewNetdataJournalFunction(config NetdataFunctionConfig, profile NetdataFunctionProfile) NetdataJournalFunction {
	if config.FunctionName == "" {
		config.FunctionName = defaultNetdataFunctionName
	}
	if config.DefaultFacets == nil {
		config.DefaultFacets = append([]string(nil), systemdDefaultFacets...)
	}
	if config.DefaultViewKeys == nil {
		config.DefaultViewKeys = append([]string(nil), systemdDefaultViewKeys...)
	}
	if config.ReaderOptions == (ReaderOptions{}) {
		config.ReaderOptions = DefaultReaderOptions().WithSnapshot(true)
	}
	if profile == nil {
		profile = SystemdJournalProfile{}
	}
	return NetdataJournalFunction{Config: config, Profile: profile}
}

func SystemdJournalNetdataFunction() NetdataJournalFunction {
	return NewNetdataJournalFunction(SystemdJournalNetdataFunctionConfig(), SystemdJournalProfile{})
}

func SystemdJournalPluginCompatibleNetdataFunction() NetdataJournalFunction {
	return NewNetdataJournalFunction(SystemdJournalNetdataFunctionConfig(), SystemdJournalPluginProfile{})
}

type NetdataFunctionProgress struct {
	CurrentFile  int
	TotalFiles   int
	MatchedFiles uint64
	SkippedFiles uint64
	Stats        ExplorerStats
	Elapsed      time.Duration
}

type NetdataJournalFileMetadata struct {
	SourceType                 *uint64
	SourceName                 string
	FileLastModifiedUsec       *uint64
	MsgFirstRealtimeUsec       *uint64
	MsgLastRealtimeUsec        *uint64
	JournalVsRealtimeDeltaUsec *uint64
}

type NetdataFunctionState interface {
	FileMetadata(path string) *NetdataJournalFileMetadata
	UpdateFileJournalVsRealtimeDeltaUsec(path string, deltaUsec uint64)
}

type NetdataFunctionRunOptions struct {
	Timeout              *time.Duration
	ProgressCallback     func(NetdataFunctionProgress)
	CancellationCallback func() bool
	State                NetdataFunctionState
	ProgressInterval     time.Duration
}

func NetdataFunctionRunOptionsFromTimeoutSeconds(seconds uint64) NetdataFunctionRunOptions {
	if seconds == 0 {
		seconds = netdataDisabledTimeoutSeconds
	}
	timeout := time.Duration(seconds) * time.Second
	return NetdataFunctionRunOptions{
		Timeout:          &timeout,
		ProgressInterval: defaultExplorerProgressInterval(),
	}
}

func DefaultNetdataFunctionRunOptions() NetdataFunctionRunOptions {
	return NetdataFunctionRunOptionsFromTimeoutSeconds(0)
}

func (f NetdataJournalFunction) RunDirectoryRequestBytes(directory string, request []byte) (map[string]any, error) {
	return f.RunDirectoryRequestBytesWithOptions(directory, request, DefaultNetdataFunctionRunOptions())
}

func (f NetdataJournalFunction) RunDirectoryRequestBytesWithOptions(directory string, request []byte, options NetdataFunctionRunOptions) (map[string]any, error) {
	var value any
	if err := json.Unmarshal(request, &value); err != nil {
		return nil, fmt.Errorf("%w: invalid Netdata function JSON: %v", errInvalidJournal, err)
	}
	object, ok := value.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("%w: Netdata function request must be a JSON object", errInvalidJournal)
	}
	return f.RunDirectoryRequestJSONWithOptions(directory, object, options)
}

func (f NetdataJournalFunction) RunDirectoryRequestJSON(directory string, request map[string]any) (map[string]any, error) {
	return f.RunDirectoryRequestJSONWithOptions(directory, request, DefaultNetdataFunctionRunOptions())
}

func (f NetdataJournalFunction) RunDirectoryRequestJSONWithOptions(directory string, request map[string]any, options NetdataFunctionRunOptions) (map[string]any, error) {
	if options.ProgressInterval == 0 {
		options.ProgressInterval = defaultExplorerProgressInterval()
	}
	parsed, err := parseNetdataRequest(request, f.Config)
	if err != nil {
		return nil, err
	}
	collection, err := collectNetdataJournalFiles(directory)
	if err != nil {
		return nil, err
	}
	if parsed.Info {
		return f.infoResponse(parsed.Echo, collection.Files, options), nil
	}
	annotationPaths := append([]string(nil), collection.Files...)
	selected := selectNetdataJournalFiles(collection.Files, parsed, f.Config.ReaderOptions, options)
	if response := notModifiedBeforeScanResponse(parsed, selected); response != nil {
		return response, nil
	}
	deadline := netdataDeadline(options)
	combined, err := f.exploreFiles(selected.Files, parsed, deadline, options)
	if err != nil {
		return nil, err
	}
	combined.SkippedFiles += collection.Skipped
	combined.FileErrors = append(combined.FileErrors, collection.Errors...)
	if !combined.Cancelled && !parsed.DataOnly {
		combined.addZeroCountFacetValuesFromFiles(parsed.Facets, f.Config.ReaderOptions)
		combined.addZeroCountSelectedFilterValues(parsed)
		if shouldCollectUnfilteredFacetVocabulary(parsed, combined) {
			vocabularyRequest := parsed.unfilteredVocabulary()
			vocabulary, err := f.exploreFiles(selected.Files, vocabularyRequest, deadline, options)
			if err != nil {
				return nil, err
			}
			combined.addZeroCountFacetValues(vocabulary.Facets)
		}
	}
	return f.queryResponse(parsed, annotationPaths, combined), nil
}

func netdataDeadline(options NetdataFunctionRunOptions) *time.Time {
	if options.Timeout == nil {
		return nil
	}
	value := time.Now().Add(*options.Timeout)
	return &value
}

type netdataRequest struct {
	Info                bool
	Echo                map[string]any
	AfterRealtimeUsec   *uint64
	BeforeRealtimeUsec  *uint64
	IfModifiedSinceUsec uint64
	Anchor              ExplorerAnchor
	Direction           Direction
	Limit               int
	DataOnly            bool
	Delta               bool
	Tail                bool
	Sampling            uint64
	SourceType          uint64
	ExactSources        []string
	Filters             []ExplorerFilter
	Facets              [][]byte
	Histogram           string
	FTSTerms            []ExplorerFtsPattern
	FTSPatterns         [][]byte
	FTSNegativePatterns [][]byte
}

func parseNetdataRequest(object map[string]any, config NetdataFunctionConfig) (netdataRequest, error) {
	nowSeconds := unixNowSeconds()
	info := getJSONBool(object, "info", false)
	after, hasAfter := getJSONInt64(object, "after")
	before, hasBefore := getJSONInt64(object, "before")
	afterUsec, beforeUsec := normalizeNetdataTimeWindow(nowSeconds, optionalI64(after, hasAfter), optionalI64(before, hasBefore))
	direction := requestDirection(object)
	ifModifiedSince := getJSONUint64(object, "if_modified_since", 0)
	dataOnly := getJSONBool(object, "data_only", false)
	delta := dataOnly && getJSONBool(object, "delta", false)
	tail := dataOnly && ifModifiedSince != 0 && getJSONBool(object, "tail", false)
	sampling := getJSONUint64(object, "sampling", defaultNetdataItemsSampling)
	anchor, direction := requestAnchorAndDirection(object, tail, direction, afterUsec, beforeUsec)
	requestedLimit := requestLimit(object)
	limit := requestedLimit
	if limit < 2 {
		limit = 2
	}
	requestedFacets, hasRequestedFacets := parseStringArray(object["facets"])
	facets := requestFacets(requestedFacets, hasRequestedFacets, config)
	requestedHistogram := requestHistogram(object)
	histogram := requestedHistogram
	if histogram == "" {
		histogram = config.DefaultHistogram
	}
	requestedQuery := getJSONString(object, "query")
	var ftsTerms []ExplorerFtsPattern
	var ftsPatterns [][]byte
	var ftsNegative [][]byte
	if requestedQuery != "" {
		ftsTerms, ftsPatterns, ftsNegative = parseFTSQueryPatterns(requestedQuery)
	}
	sourceSelection := parseSourceSelection(object["selections"])
	filters := parseNetdataFilters(object["selections"])

	echo := normalizedRequestEcho(netdataRequestEchoInput{
		Info:                info,
		AfterRealtimeUsec:   afterUsec,
		BeforeRealtimeUsec:  beforeUsec,
		IfModifiedSinceUsec: ifModifiedSince,
		Anchor:              anchor,
		Direction:           direction,
		Limit:               requestedLimit,
		DataOnly:            dataOnly,
		Delta:               delta,
		Tail:                tail,
		Sampling:            sampling,
		SourceType:          sourceSelection.SourceType,
		RequestedFacets:     requestedFacets,
		HasRequestedFacets:  hasRequestedFacets,
		Selections:          object["selections"],
		Histogram:           requestedHistogram,
		Query:               requestedQuery,
	})

	return netdataRequest{
		Info:                info,
		Echo:                echo,
		AfterRealtimeUsec:   afterUsec,
		BeforeRealtimeUsec:  beforeUsec,
		IfModifiedSinceUsec: ifModifiedSince,
		Anchor:              anchor,
		Direction:           direction,
		Limit:               limit,
		DataOnly:            dataOnly,
		Delta:               delta,
		Tail:                tail,
		Sampling:            sampling,
		SourceType:          sourceSelection.SourceType,
		ExactSources:        sourceSelection.ExactSources,
		Filters:             filters,
		Facets:              facets,
		Histogram:           histogram,
		FTSTerms:            ftsTerms,
		FTSPatterns:         ftsPatterns,
		FTSNegativePatterns: ftsNegative,
	}, nil
}

func (r netdataRequest) matchesSource(path string, metadata *NetdataJournalFileMetadata) bool {
	if r.SourceType == netdataSourceTypeAll && len(r.ExactSources) == 0 {
		return true
	}
	if r.SourceType&netdataSourceTypeAll != 0 {
		return true
	}
	sourceType := journalFileSourceType(path)
	if metadata != nil && metadata.SourceType != nil {
		sourceType = *metadata.SourceType
	}
	if sourceType&r.SourceType != 0 {
		return true
	}
	if len(r.ExactSources) == 0 {
		return false
	}
	sourceName := ""
	if metadata != nil {
		sourceName = metadata.SourceName
	}
	if sourceName == "" {
		sourceName = journalFileExactSourceName(path)
	}
	for _, exact := range r.ExactSources {
		if sourceName == exact {
			return true
		}
	}
	return false
}

func (r netdataRequest) toExplorerQuery(matchedFiles uint64, header *journalHeader, realtimeSlackUsec uint64) ExplorerQuery {
	analysisEnabled := !r.DataOnly || r.Delta
	tailAnchor := r.Tail && r.Anchor.Kind == ExplorerAnchorRealtime
	var sampling *ExplorerSampling
	if analysisEnabled && r.Sampling != 0 && matchedFiles != 0 && r.AfterRealtimeUsec != nil && r.BeforeRealtimeUsec != nil {
		var fileHeader journalHeader
		if header != nil {
			fileHeader = *header
		}
		messages := fileHeader.nEntries
		if fileHeader.headEntrySeqnum != 0 && fileHeader.tailEntrySeqnum != 0 && fileHeader.tailEntrySeqnum >= fileHeader.headEntrySeqnum {
			messages = fileHeader.tailEntrySeqnum - fileHeader.headEntrySeqnum + 1
		}
		sampling = &ExplorerSampling{
			Budget:               r.Sampling,
			MatchedFiles:         matchedFiles,
			FileHeadRealtimeUsec: fileHeader.headEntryRealtime,
			FileTailRealtimeUsec: fileHeader.tailEntryRealtime,
			FileHeadSeqnum:       fileHeader.headEntrySeqnum,
			FileTailSeqnum:       fileHeader.tailEntrySeqnum,
			FileEntries:          messages,
		}
	}
	query := DefaultExplorerQuery()
	query.AfterRealtimeUsec = r.AfterRealtimeUsec
	query.BeforeRealtimeUsec = r.BeforeRealtimeUsec
	query.Anchor = r.Anchor
	query.Direction = r.Direction
	query.Limit = r.Limit
	query.Filters = cloneExplorerFilters(r.Filters)
	if analysisEnabled {
		query.Facets = cloneByteSlices(r.Facets)
		if r.Histogram != "" {
			query.Histogram = []byte(r.Histogram)
		}
	}
	query.HistogramBuckets = netdataDefaultHistogramBuckets
	query.FTSTerms = cloneFTSTerms(r.FTSTerms)
	query.FTSPatterns = cloneByteSlices(r.FTSPatterns)
	query.FTSNegative = cloneByteSlices(r.FTSNegativePatterns)
	query.FieldMode = ExplorerFieldModeFirstValue
	query.ExcludeFacetFieldFilters = r.distinctFilterFields() > 1
	query.UseSourceRealtime = true
	query.RealtimeSlackUsec = normalizeJournalVsRealtimeDeltaUsec(realtimeSlackUsec)
	query.StopWhenRowsFull = r.DataOnly && !tailAnchor
	query.StopWhenRowsFullEvery = netdataDataOnlyCheckEveryRows
	query.Sampling = sampling
	query.DebugCollectColumnFieldsByRowTraversal = false
	return query
}

func (r netdataRequest) fileQuery(matchedFiles int, header *journalHeader, order netdataJournalFileOrderInfo) ExplorerQuery {
	query := r.toExplorerQuery(uint64(matchedFiles), header, order.JournalVsRealtimeDeltaUsec)
	if r.DataOnly && r.Delta {
		query.StopWhenRowsFull = false
	}
	return query
}

func (r netdataRequest) unfilteredVocabulary() netdataRequest {
	out := r
	out.Filters = nil
	out.Histogram = ""
	out.Limit = 0
	out.FTSTerms = nil
	out.FTSPatterns = nil
	out.FTSNegativePatterns = nil
	return out
}

func (r netdataRequest) distinctFilterFields() int {
	seen := make(map[string]struct{})
	for _, filter := range r.Filters {
		seen[string(filter.Field)] = struct{}{}
	}
	return len(seen)
}

type netdataLocatedRow struct {
	FilePath string
	Row      ExplorerRow
}

type netdataCombinedResult struct {
	Rows            []netdataLocatedRow
	Facets          map[string]map[string]uint64
	Histogram       *ExplorerHistogram
	ColumnFields    map[string]struct{}
	Stats           ExplorerStats
	PageCounters    netdataPageCounters
	HasPageCounters bool
	MatchedFiles    uint64
	MatchedPaths    []string
	SkippedFiles    uint64
	FileErrors      []string
	Partial         bool
	TimedOut        bool
	Cancelled       bool
	SamplingEnabled bool
}

func newNetdataCombinedResult() netdataCombinedResult {
	return netdataCombinedResult{
		Facets:       make(map[string]map[string]uint64),
		ColumnFields: make(map[string]struct{}),
	}
}

type netdataPageCounters struct {
	Matched uint64
	Before  uint64
	After   uint64
}

type netdataPageHeap struct {
	Values []uint64
	Max    bool
}

func (h netdataPageHeap) Len() int {
	return len(h.Values)
}

func (h netdataPageHeap) Less(i, j int) bool {
	if h.Max {
		return h.Values[i] > h.Values[j]
	}
	return h.Values[i] < h.Values[j]
}

func (h netdataPageHeap) Swap(i, j int) {
	h.Values[i], h.Values[j] = h.Values[j], h.Values[i]
}

func (h *netdataPageHeap) Push(value any) {
	typed, ok := value.(uint64)
	if !ok {
		panic("netdataPageHeap.Push expects uint64")
	}
	h.Values = append(h.Values, typed)
}

func (h *netdataPageHeap) Pop() any {
	old := h.Values
	value := old[len(old)-1]
	h.Values = old[:len(old)-1]
	return value
}

func (h netdataPageHeap) Peek() uint64 {
	if len(h.Values) == 0 {
		return 0
	}
	return h.Values[0]
}

type netdataPageWindow struct {
	Direction          Direction
	AnchorStartUsec    *uint64
	AnchorStopUsec     *uint64
	Limit              int
	Retained           netdataPageHeap
	OldestRetainedUsec *uint64
	NewestRetainedUsec *uint64
	Matched            uint64
	SkipsBefore        uint64
	SkipsAfter         uint64
	Shifts             uint64
}

func newNetdataPageWindow(request netdataRequest) *netdataPageWindow {
	var anchor *uint64
	if request.Anchor.Kind == ExplorerAnchorRealtime {
		value := request.Anchor.RealtimeUsec
		anchor = &value
	}
	return &netdataPageWindow{
		Direction:       request.Direction,
		AnchorStartUsec: anchor,
		Limit:           request.Limit,
		Retained:        netdataPageHeap{Max: request.Direction == DirectionForward},
	}
}

func (w *netdataPageWindow) candidateToKeep(realtimeUsec uint64) bool {
	if w.Limit == 0 || !w.entryWithinAnchorReadonly(realtimeUsec) {
		return false
	}
	if w.Retained.Len() < w.Limit {
		return true
	}
	return w.OldestRetainedUsec != nil && w.NewestRetainedUsec != nil &&
		realtimeUsec >= *w.OldestRetainedUsec && realtimeUsec <= *w.NewestRetainedUsec
}

func (w *netdataPageWindow) observe(realtimeUsec uint64) {
	if !w.entryWithinAnchor(realtimeUsec) || w.Limit == 0 {
		return
	}
	w.Matched++
	if w.Retained.Len() < w.Limit {
		heap.Push(&w.Retained, realtimeUsec)
		w.addRetainedBound(realtimeUsec)
		return
	}
	switch w.Direction {
	case DirectionBackward:
		oldest := w.Retained.Peek()
		if realtimeUsec < oldest {
			w.SkipsAfter++
			return
		}
		heap.Pop(&w.Retained)
		heap.Push(&w.Retained, realtimeUsec)
	case DirectionForward:
		newest := w.Retained.Peek()
		if realtimeUsec > newest {
			w.SkipsBefore++
			return
		}
		heap.Pop(&w.Retained)
		heap.Push(&w.Retained, realtimeUsec)
	}
	w.refreshRetainedBounds()
	w.Shifts++
}

func (w *netdataPageWindow) addRetainedBound(realtimeUsec uint64) {
	if w.OldestRetainedUsec == nil || realtimeUsec < *w.OldestRetainedUsec {
		value := realtimeUsec
		w.OldestRetainedUsec = &value
	}
	if w.NewestRetainedUsec == nil || realtimeUsec > *w.NewestRetainedUsec {
		value := realtimeUsec
		w.NewestRetainedUsec = &value
	}
}

func (w *netdataPageWindow) refreshRetainedBounds() {
	if w.Retained.Len() == 0 {
		w.OldestRetainedUsec = nil
		w.NewestRetainedUsec = nil
		return
	}
	oldest := w.Retained.Values[0]
	newest := w.Retained.Values[0]
	for _, value := range w.Retained.Values[1:] {
		if value < oldest {
			oldest = value
		}
		if value > newest {
			newest = value
		}
	}
	w.OldestRetainedUsec = &oldest
	w.NewestRetainedUsec = &newest
}

func (w *netdataPageWindow) entryWithinAnchor(realtimeUsec uint64) bool {
	switch w.Direction {
	case DirectionBackward:
		if w.AnchorStartUsec != nil && realtimeUsec >= *w.AnchorStartUsec {
			w.SkipsBefore++
			return false
		}
		if w.AnchorStopUsec != nil && realtimeUsec <= *w.AnchorStopUsec {
			w.SkipsAfter++
			return false
		}
	case DirectionForward:
		if w.AnchorStartUsec != nil && realtimeUsec <= *w.AnchorStartUsec {
			w.SkipsAfter++
			return false
		}
		if w.AnchorStopUsec != nil && realtimeUsec >= *w.AnchorStopUsec {
			w.SkipsBefore++
			return false
		}
	}
	return true
}

func (w *netdataPageWindow) entryWithinAnchorReadonly(realtimeUsec uint64) bool {
	switch w.Direction {
	case DirectionBackward:
		if w.AnchorStartUsec != nil && realtimeUsec >= *w.AnchorStartUsec {
			return false
		}
		if w.AnchorStopUsec != nil && realtimeUsec <= *w.AnchorStopUsec {
			return false
		}
	case DirectionForward:
		if w.AnchorStartUsec != nil && realtimeUsec <= *w.AnchorStartUsec {
			return false
		}
		if w.AnchorStopUsec != nil && realtimeUsec >= *w.AnchorStopUsec {
			return false
		}
	}
	return true
}

func (w *netdataPageWindow) counters() netdataPageCounters {
	return netdataPageCounters{
		Matched: w.Matched,
		Before:  w.SkipsBefore,
		After:   w.SkipsAfter + w.Shifts,
	}
}

func (w *netdataPageWindow) canStopDeltaFile(realtimeUsec, slackUsec uint64) bool {
	if w.Limit == 0 || w.Retained.Len() < w.Limit {
		return false
	}
	switch w.Direction {
	case DirectionBackward:
		oldest := w.Retained.Peek()
		return realtimeUsec < saturatingSub(oldest, slackUsec)
	case DirectionForward:
		newest := w.Retained.Peek()
		return realtimeUsec > saturatingAdd(newest, slackUsec)
	default:
		return false
	}
}

type netdataRealtimeAdjuster struct {
	Direction        Direction
	LastRealtimeFrom uint64
	LastRealtimeTo   uint64
}

func newNetdataRealtimeAdjuster(direction Direction) *netdataRealtimeAdjuster {
	return &netdataRealtimeAdjuster{Direction: direction}
}

func (a *netdataRealtimeAdjuster) adjust(realtimeUsec uint64) uint64 {
	if a.LastRealtimeFrom == 0 && a.LastRealtimeTo == 0 {
		a.LastRealtimeFrom = realtimeUsec
		a.LastRealtimeTo = realtimeUsec
		return realtimeUsec
	}
	if realtimeUsec >= a.LastRealtimeFrom && realtimeUsec <= a.LastRealtimeTo {
		switch a.Direction {
		case DirectionBackward:
			a.LastRealtimeFrom = saturatingSub(a.LastRealtimeFrom, 1)
			return a.LastRealtimeFrom
		default:
			a.LastRealtimeTo = saturatingAdd(a.LastRealtimeTo, 1)
			return a.LastRealtimeTo
		}
	}
	a.LastRealtimeFrom = realtimeUsec
	a.LastRealtimeTo = realtimeUsec
	return realtimeUsec
}

func (f NetdataJournalFunction) exploreFiles(files []netdataSelectedJournalFile, request netdataRequest, deadline *time.Time, options NetdataFunctionRunOptions) (netdataCombinedResult, error) {
	query := request.toExplorerQuery(uint64(len(files)), nil, netdataJournalRealtimeDeltaDefault)
	combined := newNetdataCombinedResult()
	pageWindow := newNetdataPageWindow(request)
	combined.SamplingEnabled = query.Sampling != nil
	samplingState := newExplorerSamplingState(query, histogramBucketCountForNetdataQuery(query))
	realtimeAdjuster := newNetdataRealtimeAdjuster(request.Direction)
	started := time.Now()
	totalFiles := len(files)

	for index, file := range files {
		if shouldStopBeforeFile(&combined, deadline, options) {
			break
		}
		progress := netdataProgressContext{CurrentFile: index + 1, TotalFiles: totalFiles, Started: started}
		reader, err := OpenFileWithOptions(file.Path, f.Config.ReaderOptions)
		if err != nil {
			combined.SkippedFiles++
			combined.FileErrors = append(combined.FileErrors, fmt.Sprintf("%s: %v", file.Path, err))
			emitProgressForCombined(options, combined, progress)
			continue
		}
		combined.MatchedFiles++
		combined.MatchedPaths = append(combined.MatchedPaths, file.Path)
		if !request.DataOnly {
			if fields, err := reader.EnumerateFields(); err == nil {
				combined.addColumnFields(fields)
			} else {
				combined.FileErrors = append(combined.FileErrors, fmt.Sprintf("%s: FIELD index enumeration failed: %v", file.Path, err))
			}
		}
		fileQuery := request.fileQuery(len(files), reader.Header(), file.Order)
		control := NewExplorerControl()
		control.SetDeadline(deadline)
		control.SetCancellationCallback(options.CancellationCallback)
		control.SetProgressInterval(options.ProgressInterval)
		control.SetProgressCallback(func(progress ExplorerProgress) {
			emitExplorerProgress(options, combined, progress, netdataProgressContext{CurrentFile: index + 1, TotalFiles: totalFiles, Started: started})
		})
		control.setSamplingState(samplingState)
		control.setCandidateRowCallback(func(realtimeUsec uint64) bool {
			return pageWindow.candidateToKeep(realtimeUsec)
		})
		control.setRealtimeAdjustCallback(func(realtimeUsec uint64) uint64 {
			return realtimeAdjuster.adjust(realtimeUsec)
		})
		realtimeDelta := file.Order.JournalVsRealtimeDeltaUsec
		control.SetMatchedRowCallback(func(realtimeUsec, rowsMatched uint64) bool {
			pageWindow.observe(realtimeUsec)
			return request.DataOnly && request.Delta &&
				rowsMatched%netdataDataOnlyCheckEveryRows == 0 &&
				pageWindow.canStopDeltaFile(realtimeUsec, realtimeDelta)
		})
		result, err := reader.exploreCursorRows(fileQuery, f.Config.ExplorerStrategy, control)
		stopReason := control.StopReason()
		closeErr := reader.Close()
		if err != nil {
			combined.SkippedFiles++
			combined.FileErrors = append(combined.FileErrors, fmt.Sprintf("%s: %v", file.Path, err))
			continue
		}
		if closeErr != nil {
			combined.FileErrors = append(combined.FileErrors, fmt.Sprintf("%s: close: %v", file.Path, closeErr))
		}
		updateLearnedRealtimeDelta(options, file.Path, file.Order, result.Stats)
		if err := combined.merge(file.Path, result, fileQuery.Direction, request.Limit); err != nil {
			return combined, err
		}
		emitProgressForCombined(options, combined, progress)
		if options.CancellationCallback != nil && options.CancellationCallback() {
			combined.Partial = true
			combined.Cancelled = true
			break
		}
		if stopReason != ExplorerStopNone {
			combined.Partial = true
			if stopReason == ExplorerStopTimedOut {
				combined.TimedOut = true
			}
			if stopReason == ExplorerStopCancelled {
				combined.Cancelled = true
			}
			break
		}
		if request.DataOnly && !request.Delta && !request.Tail &&
			len(combined.Rows) >= request.Limit &&
			remainingFilesCannotAffectDataPage(combined, request, files, index+1) {
			break
		}
	}
	combined.expandRowPayloads(f.Config.ReaderOptions)
	combined.PageCounters = pageWindow.counters()
	combined.HasPageCounters = true
	return combined, nil
}

func histogramBucketCountForNetdataQuery(query ExplorerQuery) int {
	if query.Histogram == nil {
		return 0
	}
	histogram := newExplorerHistogram(query.Histogram, query)
	return len(histogram.Buckets)
}

func (c *netdataCombinedResult) merge(path string, result ExplorerResult, direction Direction, limit int) error {
	if result.Histogram != nil {
		if err := mergeExplorerHistogram(&c.Histogram, *result.Histogram); err != nil {
			return err
		}
	}
	c.mergeStats(result.Stats)
	for _, row := range result.Rows {
		c.Rows = append(c.Rows, netdataLocatedRow{FilePath: path, Row: row})
	}
	for field := range result.ColumnFields {
		c.ColumnFields[field] = struct{}{}
	}
	for field, values := range result.Facets {
		target := c.Facets[field]
		if target == nil {
			target = make(map[string]uint64)
			c.Facets[field] = target
		}
		for value, count := range values {
			addNetdataFacetCount(target, value, count)
		}
	}
	c.sortAndLimit(direction, limit)
	return nil
}

func (c *netdataCombinedResult) addColumnFields(fields map[string]struct{}) {
	for field := range fields {
		c.ColumnFields[field] = struct{}{}
	}
}

func (c *netdataCombinedResult) sortAndLimit(direction Direction, limit int) {
	switch direction {
	case DirectionForward:
		sort.Slice(c.Rows, func(i, j int) bool { return c.Rows[i].Row.RealtimeUsec < c.Rows[j].Row.RealtimeUsec })
	default:
		sort.Slice(c.Rows, func(i, j int) bool { return c.Rows[i].Row.RealtimeUsec > c.Rows[j].Row.RealtimeUsec })
	}
	makeRowTimestampsUnique(c.Rows, direction)
	if limit >= 0 && len(c.Rows) > limit {
		c.Rows = c.Rows[:limit]
	}
	c.Stats.RowsReturned = uint64(len(c.Rows))
}

func (c *netdataCombinedResult) expandRowPayloads(readerOptions ReaderOptions) {
	if len(c.Rows) == 0 {
		c.Stats.RowsReturned = 0
		return
	}
	for i := range c.Rows {
		if len(c.Rows[i].Row.Payloads) != 0 {
			continue
		}
		if err := expandLocatedRowPayloads(&c.Rows[i], readerOptions); err != nil {
			c.Partial = true
			c.FileErrors = append(c.FileErrors, fmt.Sprintf("%s cursor %s: %v", c.Rows[i].FilePath, c.Rows[i].Row.Cursor, err))
			continue
		}
		c.Stats.ReturnedRowExpansions++
	}
	c.Stats.RowsReturned = uint64(len(c.Rows))
}

func (c *netdataCombinedResult) mergeStats(stats ExplorerStats) {
	c.Stats.RowsExamined += stats.RowsExamined
	c.Stats.RowsMatched += stats.RowsMatched
	c.Stats.FacetRowsMatched += stats.FacetRowsMatched
	c.Stats.RowsReturned += stats.RowsReturned
	c.Stats.RowsUnsampled += stats.RowsUnsampled
	c.Stats.RowsEstimated += stats.RowsEstimated
	c.Stats.SamplingSampled += stats.SamplingSampled
	c.Stats.SamplingUnsampled += stats.SamplingUnsampled
	c.Stats.SamplingEstimated += stats.SamplingEstimated
	if stats.LastRealtimeUsec > c.Stats.LastRealtimeUsec {
		c.Stats.LastRealtimeUsec = stats.LastRealtimeUsec
	}
	if stats.MaxSourceRealtimeDeltaUsec > c.Stats.MaxSourceRealtimeDeltaUsec {
		c.Stats.MaxSourceRealtimeDeltaUsec = stats.MaxSourceRealtimeDeltaUsec
	}
	c.Stats.DataRefsSeen += stats.DataRefsSeen
	c.Stats.DataRefsSkipped += stats.DataRefsSkipped
	c.Stats.DataPayloadsLoaded += stats.DataPayloadsLoaded
	c.Stats.DataObjectsClassified += stats.DataObjectsClassified
	c.Stats.DataCacheHits += stats.DataCacheHits
	c.Stats.DataCacheMisses += stats.DataCacheMisses
	c.Stats.PayloadsDecompressed += stats.PayloadsDecompressed
	c.Stats.FTSScans += stats.FTSScans
	c.Stats.FacetUpdates += stats.FacetUpdates
	c.Stats.HistogramUpdates += stats.HistogramUpdates
	c.Stats.ReturnedRowExpansions += stats.ReturnedRowExpansions
	c.Stats.EarlyStopOpportunities += stats.EarlyStopOpportunities
	c.Stats.EarlyStops += stats.EarlyStops
}

func (c *netdataCombinedResult) addZeroCountFacetValues(vocabulary map[string]map[string]uint64) {
	for field, values := range vocabulary {
		target := c.Facets[field]
		if target == nil {
			target = make(map[string]uint64)
			c.Facets[field] = target
		}
		for value := range values {
			addNetdataFacetCount(target, value, 0)
		}
	}
}

func (c *netdataCombinedResult) addZeroCountFacetValuesFromFiles(fields [][]byte, readerOptions ReaderOptions) {
	for _, path := range c.MatchedPaths {
		func() {
			reader, err := openFileWithOptions(path, readerOptions, false)
			if err != nil {
				return
			}
			defer reader.Close()
			for _, field := range fields {
				fieldName := string(field)
				values, err := reader.QueryUnique(fieldName)
				if err != nil || len(values) == 0 {
					continue
				}
				target := c.Facets[fieldName]
				if target == nil {
					target = make(map[string]uint64)
					c.Facets[fieldName] = target
				}
				for _, value := range values {
					addNetdataFacetCount(target, string(value), 0)
				}
			}
		}()
	}
}

func (c *netdataCombinedResult) addZeroCountSelectedFilterValues(request netdataRequest) {
	reportFields := make(map[string]struct{})
	for _, field := range request.Facets {
		reportFields[string(field)] = struct{}{}
	}
	if request.Histogram != "" {
		reportFields[request.Histogram] = struct{}{}
	}
	for _, filter := range request.Filters {
		field := string(filter.Field)
		if _, ok := reportFields[field]; !ok {
			continue
		}
		target := c.Facets[field]
		if target == nil {
			target = make(map[string]uint64)
			c.Facets[field] = target
		}
		for _, value := range filter.Values {
			addNetdataFacetCount(target, string(value), 0)
		}
	}
}

func (c netdataCombinedResult) reportableFacetFields(requested [][]byte) []string {
	var fields []string
	for _, field := range requested {
		fieldName := string(field)
		if values, ok := c.Facets[fieldName]; ok && facetGroupIsReportable(values) {
			fields = append(fields, fieldName)
		}
	}
	return fields
}

func expandLocatedRowPayloads(row *netdataLocatedRow, readerOptions ReaderOptions) error {
	reader, err := OpenFileWithOptions(row.FilePath, readerOptions)
	if err != nil {
		return err
	}
	defer reader.Close()
	if err := reader.SeekCursor(row.Row.Cursor); err != nil {
		return err
	}
	entry, err := reader.GetEntry()
	if err != nil {
		return err
	}
	row.Row.Payloads = entry.Payloads
	return nil
}

func mergeExplorerHistogram(target **ExplorerHistogram, source ExplorerHistogram) error {
	if *target == nil {
		copyHistogram := source
		*target = &copyHistogram
		return nil
	}
	if !bytes.Equal((*target).Field, source.Field) || len((*target).Buckets) != len(source.Buckets) {
		return fmt.Errorf("%w: inconsistent Netdata histogram bucket shape", ErrUnsupported)
	}
	for i := range source.Buckets {
		if (*target).Buckets[i].StartRealtimeUsec != source.Buckets[i].StartRealtimeUsec ||
			(*target).Buckets[i].EndRealtimeUsec != source.Buckets[i].EndRealtimeUsec {
			return fmt.Errorf("%w: inconsistent Netdata histogram bucket shape", ErrUnsupported)
		}
		for value, count := range source.Buckets[i].Values {
			(*target).Buckets[i].Values[value] += count
		}
	}
	return nil
}

func netdataFunctionError(status uint64, message string) map[string]any {
	return map[string]any{"status": status, "errorMessage": message}
}

func notModifiedBeforeScanResponse(request netdataRequest, selected netdataSelectedJournalFiles) map[string]any {
	if request.IfModifiedSinceUsec != 0 && !selected.FilesAreNewer {
		return netdataFunctionError(304, "No new data since the previous call.")
	}
	return nil
}

func shouldCollectUnfilteredFacetVocabulary(request netdataRequest, combined netdataCombinedResult) bool {
	return !request.DataOnly && !combined.Partial && len(request.Filters) != 0
}

func (f NetdataJournalFunction) queryResponse(request netdataRequest, annotationPaths []string, combined netdataCombinedResult) map[string]any {
	notModified := request.IfModifiedSinceUsec != 0 && !combined.Partial && combined.Stats.RowsMatched == 0
	if combined.Cancelled {
		return netdataFunctionError(499, "Request cancelled.")
	}
	if notModified {
		return netdataFunctionError(304, "No new data since the previous call.")
	}
	artifacts := f.queryResponseArtifacts(request, annotationPaths, combined)
	response := map[string]any{
		"_request": request.Echo,
		"versions": map[string]any{
			"netdata_function_api": 1,
			"sdk":                  "go",
		},
		"_journal_files": map[string]any{
			"matched": combined.MatchedFiles,
			"skipped": combined.SkippedFiles,
			"errors":  combined.FileErrors,
		},
		"status":      200,
		"partial":     combined.Partial,
		"type":        "table",
		"show_ids":    true,
		"has_history": true,
		"pagination": map[string]any{
			"enabled": true,
			"key":     "anchor",
			"column":  "timestamp",
			"units":   "timestamp_usec",
		},
		"columns": artifacts.Columns.Map,
		"data":    artifacts.Data,
		"_stats": map[string]any{
			"sdk_explorer": combined.Stats,
		},
		"expires": netdataExpires(request),
	}
	if !request.DataOnly {
		response["message"] = queryMessage(combined.TimedOut, combined.Stats)
		response["update_every"] = 1
		response["help"] = nil
		response["accepted_params"] = f.acceptedParamsFromFields(artifacts.ReportableFacetFieldNames)
		response["default_sort_column"] = "timestamp"
		response["default_charts"] = []any{}
		response["available_histograms"] = f.availableHistograms(request, combined)
	} else if request.Histogram != "" {
		response["available_histograms"] = f.availableHistograms(request, combined)
	}
	if !request.DataOnly || request.Tail {
		response["last_modified"] = combined.Stats.LastRealtimeUsec
	}
	if combined.SamplingEnabled {
		response["_sampling"] = map[string]any{
			"enabled":   true,
			"sampled":   combined.Stats.SamplingSampled,
			"unsampled": combined.Stats.SamplingUnsampled,
			"estimated": combined.Stats.SamplingEstimated,
		}
	}
	if !request.DataOnly || request.Delta {
		facetsKey, histogramKey, itemsKey := responseAnalysisKeys(request.DataOnly)
		response[facetsKey] = artifacts.Facets
		if artifacts.Histogram != nil {
			response[histogramKey] = artifacts.Histogram
		} else {
			response[histogramKey] = nil
		}
		response[itemsKey] = artifacts.Items
	}
	return response
}

type netdataColumns struct {
	Order []string
	Map   map[string]any
}

type netdataQueryResponseArtifacts struct {
	ReportableFacetFieldNames []string
	Columns                   netdataColumns
	Data                      []any
	Facets                    []any
	Histogram                 any
	Items                     map[string]any
}

func (f NetdataJournalFunction) queryResponseArtifacts(request netdataRequest, annotationPaths []string, combined netdataCombinedResult) netdataQueryResponseArtifacts {
	reportableFacetFields := combined.reportableFacetFields(request.Facets)
	columns := f.buildColumns(request, reportableFacetFields, combined)
	bootIDs := responseBootIDs(columns.Order, combined.Rows, combined.Facets, combined.Histogram)
	context := newDisplayContext()
	context.BootFirstRealtime = collectBootFirstRealtime(annotationPaths, f.Config.ReaderOptions, bootIDs)
	data := f.buildDataRows(context, columns.Order, combined.Rows, request.Direction)
	facets := f.buildFacets(context, stringsToBytes(reportableFacetFields), combined.Facets)
	var histogram any
	if combined.Histogram != nil {
		histogram = f.buildHistogram(context, combined.Histogram, combined.Facets[string(combined.Histogram.Field)])
	}
	return netdataQueryResponseArtifacts{
		ReportableFacetFieldNames: reportableFacetFields,
		Columns:                   columns,
		Data:                      data,
		Facets:                    facets,
		Histogram:                 histogram,
		Items:                     responseItems(request, combined, uint64(len(data))),
	}
}

func (f NetdataJournalFunction) buildColumns(request netdataRequest, reportableFacetFields []string, combined netdataCombinedResult) netdataColumns {
	order := []string{"timestamp", "rowOptions"}
	pushUniqueMany(&order, f.Config.DefaultViewKeys)
	pushUniqueMany(&order, reportableFacetFields)
	if request.Histogram != "" {
		pushUnique(&order, request.Histogram)
	}
	columnFields := make([]string, 0, len(combined.ColumnFields))
	for field := range combined.ColumnFields {
		columnFields = append(columnFields, field)
	}
	sort.Strings(columnFields)
	for _, field := range columnFields {
		pushUnique(&order, field)
	}
	facetFields := make([]string, 0, len(combined.Facets))
	for field, values := range combined.Facets {
		if facetGroupIsReportable(values) {
			facetFields = append(facetFields, field)
		}
	}
	sort.Strings(facetFields)
	for _, field := range facetFields {
		pushUnique(&order, field)
	}
	for _, row := range combined.Rows {
		fields := rowFields(row)
		rowFieldNames := make([]string, 0, len(fields))
		for field := range fields {
			rowFieldNames = append(rowFieldNames, field)
		}
		sort.Strings(rowFieldNames)
		for _, field := range rowFieldNames {
			pushUnique(&order, field)
		}
	}
	columnMap := make(map[string]any, len(order))
	for index, key := range order {
		columnMap[key] = columnMetadata(key, index)
	}
	return netdataColumns{Order: order, Map: columnMap}
}

func (f NetdataJournalFunction) buildDataRows(context *DisplayContext, columnOrder []string, rows []netdataLocatedRow, direction Direction) []any {
	rowOrder := rows
	if direction == DirectionForward {
		rowOrder = make([]netdataLocatedRow, len(rows))
		for i := range rows {
			rowOrder[i] = rows[len(rows)-1-i]
		}
	}
	out := make([]any, 0, len(rowOrder))
	for _, located := range rowOrder {
		fields := rowFields(located)
		row := make([]any, 0, len(columnOrder))
		for _, column := range columnOrder {
			var value any
			switch column {
			case "timestamp":
				value = located.Row.RealtimeUsec
			case "rowOptions":
				value = f.Profile.RowOptions(fields)
			default:
				if fieldValue, ok := firstFieldValue(fields, column); ok {
					value = f.Profile.FieldDisplayValue(context, DisplayScopeData, column, fieldValue)
				} else {
					value = nil
				}
			}
			row = append(row, value)
		}
		out = append(out, row)
	}
	return out
}

func (f NetdataJournalFunction) buildFacets(context *DisplayContext, requested [][]byte, facets map[string]map[string]uint64) []any {
	out := make([]any, 0, len(requested))
	for order, fieldBytes := range requested {
		field := string(fieldBytes)
		values := facets[field]
		options := make([]map[string]any, 0, len(values))
		for value, count := range values {
			if (value == "" || value == "-") && !(count == 0 && value == "") {
				continue
			}
			if count == 0 && value == "" {
				options = append(options, map[string]any{
					"id":    netdataEmptyStringFacetHashID,
					"name":  netdataUnavailableFieldLabel,
					"count": count,
				})
				continue
			}
			raw := []byte(value)
			options = append(options, map[string]any{
				"id":    string(raw),
				"name":  f.Profile.FacetOptionName(context, field, raw),
				"count": count,
			})
		}
		sortFacetOptions(field, options)
		for i := range options {
			options[i]["order"] = i + 1
		}
		optionValues := make([]any, 0, len(options))
		for _, option := range options {
			optionValues = append(optionValues, option)
		}
		out = append(out, map[string]any{
			"id":      field,
			"name":    field,
			"order":   order + 1,
			"options": optionValues,
		})
	}
	return out
}

func (f NetdataJournalFunction) buildHistogram(context *DisplayContext, histogram *ExplorerHistogram, knownValues map[string]uint64) any {
	field := string(histogram.Field)
	dimensionSet := make(map[string]struct{})
	actualSet := make(map[string]struct{})
	bucketValues := make([]map[string]uint64, 0, len(histogram.Buckets))
	for _, bucket := range histogram.Buckets {
		values := make(map[string]uint64)
		for value, count := range bucket.Values {
			truncated := netdataFacetValue(value)
			values[truncated] += count
		}
		for value := range values {
			dimensionSet[value] = struct{}{}
			actualSet[value] = struct{}{}
		}
		bucketValues = append(bucketValues, values)
	}
	for value := range knownValues {
		if value == "" || value == "-" {
			continue
		}
		dimensionSet[value] = struct{}{}
	}
	dimensions := make([]string, 0, len(dimensionSet))
	for value := range dimensionSet {
		dimensions = append(dimensions, value)
	}
	sort.Strings(dimensions)
	labels := make([]any, 0, len(dimensions)+1)
	labels = append(labels, "time")
	for _, value := range dimensions {
		labels = append(labels, displayValueString(f.Profile.FieldDisplayValue(context, DisplayScopeHistogram, field, []byte(value))))
	}
	data := make([]any, 0, len(histogram.Buckets))
	for i, bucket := range histogram.Buckets {
		point := make([]any, 0, len(dimensions)+1)
		point = append(point, bucket.StartRealtimeUsec/1000)
		for _, value := range dimensions {
			count, ok := bucketValues[i][value]
			if ok {
				point = append(point, []any{count, 0, 0})
			} else if _, actual := actualSet[value]; actual {
				point = append(point, []any{0, 0, 0})
			} else {
				point = append(point, []any{nil, 0, 0})
			}
		}
		data = append(data, point)
	}
	return map[string]any{
		"id":   field,
		"name": field,
		"chart": map[string]any{
			"result": map[string]any{
				"labels": labels,
				"point":  map[string]any{"value": 0, "arp": 1, "pa": 2},
				"data":   data,
			},
			"view": map[string]any{
				"title":        fmt.Sprintf("Events Distribution by %s", field),
				"update_every": histogramUpdateEverySeconds(histogram),
				"units":        "events",
				"chart_type":   "stackedBar",
			},
		},
	}
}

func (f NetdataJournalFunction) infoResponse(echo map[string]any, paths []string, options NetdataFunctionRunOptions) map[string]any {
	return map[string]any{
		"_request": echo,
		"versions": map[string]any{
			"netdata_function_api": 1,
			"sdk":                  "go",
		},
		"v":               3,
		"accepted_params": f.acceptedParamsFromFields(nil),
		"required_params": f.requiredSourceParams(paths, options),
		"show_ids":        true,
		"has_history":     true,
		"pagination": map[string]any{
			"enabled": true,
			"key":     "anchor",
			"column":  "timestamp",
			"units":   "timestamp_usec",
		},
		"status": 200,
		"type":   "table",
		"help":   "Netdata-compatible journal log function backed by the systemd journal SDK",
	}
}

func (f NetdataJournalFunction) acceptedParamsFromFields(fields []string) []any {
	out := make([]any, 0, len(netdataAcceptedParams)+len(fields))
	for _, field := range netdataAcceptedParams {
		out = append(out, field)
	}
	for _, field := range fields {
		out = append(out, field)
	}
	return out
}

func (f NetdataJournalFunction) requiredSourceParams(paths []string, options NetdataFunctionRunOptions) []any {
	all := netdataJournalSourceSummary{}
	local := netdataJournalSourceSummary{}
	localNamespaces := netdataJournalSourceSummary{}
	localSystem := netdataJournalSourceSummary{}
	localUser := netdataJournalSourceSummary{}
	remote := netdataJournalSourceSummary{}
	other := netdataJournalSourceSummary{}
	exact := make(map[string]*netdataJournalSourceSummary)

	for _, path := range paths {
		metadata := fileMetadata(options, path)
		sourceType := journalFileSourceType(path)
		if metadata != nil && metadata.SourceType != nil {
			sourceType = *metadata.SourceType
		}
		all.addPath(path, f.Config.ReaderOptions, metadata)
		if sourceType&netdataSourceTypeLocalAll != 0 {
			local.addPath(path, f.Config.ReaderOptions, metadata)
		}
		if sourceType&netdataSourceTypeLocalNamespace != 0 {
			localNamespaces.addPath(path, f.Config.ReaderOptions, metadata)
		}
		if sourceType&netdataSourceTypeLocalSystem != 0 {
			localSystem.addPath(path, f.Config.ReaderOptions, metadata)
		}
		if sourceType&netdataSourceTypeLocalUser != 0 {
			localUser.addPath(path, f.Config.ReaderOptions, metadata)
		}
		if sourceType&netdataSourceTypeRemoteAll != 0 {
			remote.addPath(path, f.Config.ReaderOptions, metadata)
		}
		if sourceType&netdataSourceTypeLocalOther != 0 {
			other.addPath(path, f.Config.ReaderOptions, metadata)
		}
		sourceName := ""
		if metadata != nil {
			sourceName = metadata.SourceName
		}
		if sourceName == "" {
			sourceName = journalFileExactSourceName(path)
		}
		if sourceName != "" {
			summary := exact[sourceName]
			if summary == nil {
				summary = &netdataJournalSourceSummary{}
				exact[sourceName] = summary
			}
			summary.addPath(path, f.Config.ReaderOptions, metadata)
		}
	}
	optionsOut := make([]any, 0)
	pushSourceOption(&optionsOut, "all", all)
	pushSourceOption(&optionsOut, "all-local-logs", local)
	pushSourceOption(&optionsOut, "all-local-namespaces", localNamespaces)
	pushSourceOption(&optionsOut, "all-local-system-logs", localSystem)
	pushSourceOption(&optionsOut, "all-local-user-logs", localUser)
	pushSourceOption(&optionsOut, "all-remote-systems", remote)
	pushSourceOption(&optionsOut, "all-uncategorized", other)
	exactNames := make([]string, 0, len(exact))
	for name := range exact {
		exactNames = append(exactNames, name)
	}
	sort.Strings(exactNames)
	for _, name := range exactNames {
		pushSourceOption(&optionsOut, name, *exact[name])
	}
	return []any{map[string]any{
		"id":      "__logs_sources",
		"name":    "Journal Sources",
		"help":    "Select the logs source to query",
		"type":    "multiselect",
		"options": optionsOut,
	}}
}

func (f NetdataJournalFunction) availableHistograms(request netdataRequest, combined netdataCombinedResult) []any {
	fields := combined.reportableFacetFields(request.Facets)
	if request.DataOnly && request.Histogram != "" {
		pushUnique(&fields, request.Histogram)
	}
	sorted := append([]string(nil), fields...)
	sort.Slice(sorted, func(i, j int) bool { return netdataReorderKey(sorted[i]) < netdataReorderKey(sorted[j]) })
	orderByField := make(map[string]int, len(sorted))
	for i, field := range sorted {
		orderByField[field] = i + 1
	}
	out := make([]any, 0, len(fields))
	for _, field := range fields {
		out = append(out, map[string]any{
			"id":    field,
			"name":  field,
			"order": orderByField[field],
		})
	}
	return out
}

func responseItems(request netdataRequest, combined netdataCombinedResult, returned uint64) map[string]any {
	unsampled := combined.Stats.RowsUnsampled
	estimated := combined.Stats.RowsEstimated
	pageCounters := combined.PageCounters
	if !combined.HasPageCounters {
		pageCounters = netdataPageCounters{
			Matched: combined.Stats.RowsMatched,
			After:   responseFallbackRowsAfterReturned(combined.Stats, returned),
		}
	}
	return map[string]any{
		"evaluated":     combined.Stats.RowsExamined + unsampled + estimated,
		"matched":       pageCounters.Matched + unsampled + estimated,
		"unsampled":     unsampled,
		"estimated":     estimated,
		"returned":      returned,
		"max_to_return": uint64(request.Limit),
		"before":        pageCounters.Before,
		"after":         pageCounters.After,
	}
}

func responseFallbackRowsAfterReturned(stats ExplorerStats, returned uint64) uint64 {
	sourceRows := stats.RowsMatched
	if stats.RowsUnsampled != 0 || stats.RowsEstimated != 0 {
		sourceRows = stats.RowsExamined
	}
	return saturatingSub(sourceRows, returned)
}

func responseAnalysisKeys(dataOnly bool) (string, string, string) {
	if dataOnly {
		return "facets_delta", "histogram_delta", "items_delta"
	}
	return "facets", "histogram", "items"
}

func netdataExpires(request netdataRequest) any {
	if request.DataOnly {
		return unixNowSeconds() + 3600
	}
	return 0
}

func queryMessage(timedOut bool, stats ExplorerStats) any {
	if !timedOut && stats.RowsUnsampled == 0 && stats.RowsEstimated == 0 {
		return "OK"
	}
	total := stats.RowsExamined + stats.RowsUnsampled + stats.RowsEstimated
	if total == 0 {
		total = 1
	}
	realPercent := float64(stats.RowsExamined) * 100 / float64(total)
	unsampledPercent := float64(stats.RowsUnsampled) * 100 / float64(total)
	estimatedPercent := float64(stats.RowsEstimated) * 100 / float64(total)
	title := ""
	description := ""
	status := "notice"
	if timedOut {
		title += "Query timed-out, incomplete data. "
		description += "QUERY TIMEOUT: The query timed out and may not include all the data of the selected window. "
		status = "warning"
	}
	if stats.RowsUnsampled != 0 || stats.RowsEstimated != 0 {
		title += fmt.Sprintf("%.2f%% real data", realPercent)
		description += fmt.Sprintf("ACTUAL DATA: The filters counters reflect %.2f%% of the data. ", realPercent)
	}
	if stats.RowsUnsampled != 0 {
		title += fmt.Sprintf(", %.2f%% unsampled", unsampledPercent)
		description += fmt.Sprintf("UNSAMPLED DATA: %.2f%% of the events exist and have been counted, but their values have not been evaluated, so they are not included in the filters counters. ", unsampledPercent)
	}
	if stats.RowsEstimated != 0 {
		title += fmt.Sprintf(", %.2f%% estimated", estimatedPercent)
		description += fmt.Sprintf("ESTIMATED DATA: The query selected a large amount of data, so to avoid delaying too much, the presented data are estimated by %.2f%%. ", estimatedPercent)
	}
	return map[string]any{"title": title, "status": status, "description": description}
}

type netdataProgressContext struct {
	CurrentFile int
	TotalFiles  int
	Started     time.Time
}

func emitProgressForCombined(options NetdataFunctionRunOptions, combined netdataCombinedResult, context netdataProgressContext) {
	if options.ProgressCallback == nil {
		return
	}
	options.ProgressCallback(NetdataFunctionProgress{
		CurrentFile:  context.CurrentFile,
		TotalFiles:   context.TotalFiles,
		MatchedFiles: combined.MatchedFiles,
		SkippedFiles: combined.SkippedFiles,
		Stats:        combined.Stats,
		Elapsed:      time.Since(context.Started),
	})
}

func emitExplorerProgress(options NetdataFunctionRunOptions, combined netdataCombinedResult, progress ExplorerProgress, context netdataProgressContext) {
	if options.ProgressCallback == nil {
		return
	}
	stats := combined.Stats
	stats.RowsExamined += progress.Stats.RowsExamined
	stats.RowsMatched += progress.Stats.RowsMatched
	stats.FacetRowsMatched += progress.Stats.FacetRowsMatched
	stats.RowsReturned += progress.Stats.RowsReturned
	stats.RowsUnsampled += progress.Stats.RowsUnsampled
	stats.RowsEstimated += progress.Stats.RowsEstimated
	options.ProgressCallback(NetdataFunctionProgress{
		CurrentFile:  context.CurrentFile,
		TotalFiles:   context.TotalFiles,
		MatchedFiles: combined.MatchedFiles,
		SkippedFiles: combined.SkippedFiles,
		Stats:        stats,
		Elapsed:      time.Since(context.Started),
	})
}

func shouldStopBeforeFile(combined *netdataCombinedResult, deadline *time.Time, options NetdataFunctionRunOptions) bool {
	if options.CancellationCallback != nil && options.CancellationCallback() {
		combined.Partial = true
		combined.Cancelled = true
		return true
	}
	if deadline != nil && !time.Now().Before(*deadline) {
		combined.Partial = true
		combined.TimedOut = true
		return true
	}
	return false
}

func updateLearnedRealtimeDelta(options NetdataFunctionRunOptions, path string, order netdataJournalFileOrderInfo, stats ExplorerStats) {
	learned := stats.MaxSourceRealtimeDeltaUsec
	if learned == 0 || learned <= order.JournalVsRealtimeDeltaUsec {
		return
	}
	learned = normalizeJournalVsRealtimeDeltaUsec(learned)
	if learned <= order.JournalVsRealtimeDeltaUsec || options.State == nil {
		return
	}
	options.State.UpdateFileJournalVsRealtimeDeltaUsec(path, learned)
}

func normalizeJournalVsRealtimeDeltaUsec(deltaUsec uint64) uint64 {
	if deltaUsec < netdataJournalRealtimeDeltaDefault {
		return netdataJournalRealtimeDeltaDefault
	}
	if deltaUsec > netdataJournalRealtimeDeltaMax {
		return netdataJournalRealtimeDeltaMax
	}
	return deltaUsec
}

type netdataSelectedJournalFile struct {
	Path  string
	Order netdataJournalFileOrderInfo
}

type netdataSelectedJournalFiles struct {
	Files         []netdataSelectedJournalFile
	FilesAreNewer bool
}

func selectNetdataJournalFiles(paths []string, request netdataRequest, readerOptions ReaderOptions, options NetdataFunctionRunOptions) netdataSelectedJournalFiles {
	selected := make([]netdataSelectedJournalFile, 0, len(paths))
	for _, path := range paths {
		metadata := fileMetadata(options, path)
		if !request.matchesSource(path, metadata) {
			continue
		}
		order := journalFileOrderInfo(path, readerOptions, metadata)
		if !journalFileOrderMayOverlapRequest(order, request) {
			continue
		}
		selected = append(selected, netdataSelectedJournalFile{Path: path, Order: order})
	}
	sort.Slice(selected, func(i, j int) bool {
		cmp := compareJournalFileOrder(selected[i].Order, selected[j].Order, request.Direction)
		if cmp == 0 {
			return selected[i].Path < selected[j].Path
		}
		return cmp < 0
	})
	filesAreNewer := false
	for _, file := range selected {
		if file.Order.MsgLastRealtimeUsec > request.IfModifiedSinceUsec {
			filesAreNewer = true
			break
		}
	}
	return netdataSelectedJournalFiles{Files: selected, FilesAreNewer: filesAreNewer}
}

func remainingFilesCannotAffectDataPage(combined netdataCombinedResult, request netdataRequest, files []netdataSelectedJournalFile, nextFileIndex int) bool {
	if nextFileIndex >= len(files) {
		return true
	}
	next := files[nextFileIndex]
	if len(combined.Rows) == 0 {
		return false
	}
	switch request.Direction {
	case DirectionBackward:
		oldest := combined.Rows[0].Row.RealtimeUsec
		for _, row := range combined.Rows[1:] {
			if row.Row.RealtimeUsec < oldest {
				oldest = row.Row.RealtimeUsec
			}
		}
		return next.Order.MsgLastRealtimeUsec < saturatingSub(oldest, next.Order.JournalVsRealtimeDeltaUsec)
	default:
		newest := combined.Rows[0].Row.RealtimeUsec
		for _, row := range combined.Rows[1:] {
			if row.Row.RealtimeUsec > newest {
				newest = row.Row.RealtimeUsec
			}
		}
		return next.Order.MsgFirstRealtimeUsec > saturatingAdd(newest, next.Order.JournalVsRealtimeDeltaUsec)
	}
}

func journalFileOrderMayOverlapRequest(info netdataJournalFileOrderInfo, request netdataRequest) bool {
	if info.MsgLastRealtimeUsec == 0 {
		return true
	}
	first := saturatingSub(info.MsgFirstRealtimeUsec, netdataJournalRealtimeDeltaMax)
	last := saturatingAdd(info.MsgLastRealtimeUsec, netdataJournalRealtimeDeltaMax)
	if request.AfterRealtimeUsec != nil && last < *request.AfterRealtimeUsec {
		return false
	}
	if request.BeforeRealtimeUsec != nil && first > *request.BeforeRealtimeUsec {
		return false
	}
	return true
}

type netdataJournalFileOrderInfo struct {
	MsgFirstRealtimeUsec       uint64
	MsgLastRealtimeUsec        uint64
	FileLastModifiedUsec       uint64
	JournalVsRealtimeDeltaUsec uint64
}

func journalFileOrderInfo(path string, readerOptions ReaderOptions, metadata *NetdataJournalFileMetadata) netdataJournalFileOrderInfo {
	fileLastModified := fileModifiedUsec(path)
	if metadata != nil && metadata.FileLastModifiedUsec != nil {
		fileLastModified = *metadata.FileLastModifiedUsec
	}
	realtimeDelta := netdataJournalRealtimeDeltaDefault
	if metadata != nil && metadata.JournalVsRealtimeDeltaUsec != nil {
		realtimeDelta = normalizeJournalVsRealtimeDeltaUsec(*metadata.JournalVsRealtimeDeltaUsec)
	}
	info := netdataJournalFileOrderInfo{
		MsgFirstRealtimeUsec:       0,
		MsgLastRealtimeUsec:        fileLastModified,
		FileLastModifiedUsec:       fileLastModified,
		JournalVsRealtimeDeltaUsec: realtimeDelta,
	}
	reader, err := openFileWithOptions(path, readerOptions, false)
	if err != nil {
		return info
	}
	defer reader.Close()
	header := reader.Header()
	info.MsgFirstRealtimeUsec = header.headEntryRealtime
	if header.tailEntryRealtime != 0 {
		info.MsgLastRealtimeUsec = header.tailEntryRealtime
	}
	if metadata != nil {
		if metadata.MsgFirstRealtimeUsec != nil {
			info.MsgFirstRealtimeUsec = *metadata.MsgFirstRealtimeUsec
		}
		if metadata.MsgLastRealtimeUsec != nil {
			info.MsgLastRealtimeUsec = *metadata.MsgLastRealtimeUsec
		}
	}
	return info
}

func compareJournalFileOrder(left, right netdataJournalFileOrderInfo, direction Direction) int {
	cmp := compareU64Desc(right.MsgLastRealtimeUsec, left.MsgLastRealtimeUsec)
	if cmp == 0 {
		cmp = compareU64Desc(right.FileLastModifiedUsec, left.FileLastModifiedUsec)
	}
	if cmp == 0 {
		cmp = compareU64Desc(right.MsgFirstRealtimeUsec, left.MsgFirstRealtimeUsec)
	}
	if direction == DirectionForward {
		return -cmp
	}
	return cmp
}

func compareU64Desc(left, right uint64) int {
	if left < right {
		return -1
	}
	if left > right {
		return 1
	}
	return 0
}

func fileMetadata(options NetdataFunctionRunOptions, path string) *NetdataJournalFileMetadata {
	if options.State == nil {
		return nil
	}
	return options.State.FileMetadata(path)
}

type netdataJournalFileCollection struct {
	Files   []string
	Skipped uint64
	Errors  []string
}

func collectNetdataJournalFiles(path string) (netdataJournalFileCollection, error) {
	info, err := os.Stat(path)
	if err != nil {
		return netdataJournalFileCollection{}, err
	}
	if !info.IsDir() {
		return netdataJournalFileCollection{}, fmt.Errorf("%w: not a directory: %s", errInvalidJournal, path)
	}
	var collection netdataJournalFileCollection
	pending := []struct {
		Path  string
		Depth int
	}{{Path: path}}
	visited := make(map[string]struct{})
	for len(pending) != 0 {
		item := pending[0]
		pending = pending[1:]
		key, err := filepath.EvalSymlinks(item.Path)
		if err != nil {
			key = item.Path
		}
		if _, ok := visited[key]; ok {
			continue
		}
		if len(visited) >= netdataMaxDirectoryScanCount {
			collection.Skipped++
			collection.Errors = append(collection.Errors, fmt.Sprintf("%s: directory scan limit reached", item.Path))
			continue
		}
		visited[key] = struct{}{}
		entries, err := os.ReadDir(item.Path)
		if err != nil {
			if item.Path == path {
				return collection, err
			}
			collection.Skipped++
			collection.Errors = append(collection.Errors, fmt.Sprintf("%s: %v", item.Path, err))
			continue
		}
		for _, entry := range entries {
			entryPath := filepath.Join(item.Path, entry.Name())
			if entry.Type().IsRegular() && isJournalFileName(entry.Name()) {
				collection.Files = append(collection.Files, entryPath)
				continue
			}
			if item.Depth < netdataMaxDirectoryScanDepth && entry.IsDir() {
				pending = append(pending, struct {
					Path  string
					Depth int
				}{Path: entryPath, Depth: item.Depth + 1})
			}
		}
	}
	sort.Strings(collection.Files)
	collection.Files = dedupNetdataJournalFiles(collection.Files)
	return collection, nil
}

func dedupNetdataJournalFiles(files []string) []string {
	seen := make(map[string]struct{})
	out := files[:0]
	for _, path := range files {
		key, err := filepath.EvalSymlinks(path)
		if err != nil {
			key = path
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, path)
	}
	return out
}

type netdataJournalSourceSummary struct {
	Files             uint64
	TotalSize         uint64
	FirstRealtimeUsec *uint64
	LastRealtimeUsec  *uint64
}

func (s *netdataJournalSourceSummary) addPath(path string, readerOptions ReaderOptions, metadata *NetdataJournalFileMetadata) {
	if info, err := os.Stat(path); err == nil {
		s.Files++
		s.TotalSize += uint64(info.Size())
	}
	if metadata != nil {
		if metadata.MsgFirstRealtimeUsec != nil {
			s.addFirst(*metadata.MsgFirstRealtimeUsec)
		}
		if metadata.MsgLastRealtimeUsec != nil {
			s.addLast(*metadata.MsgLastRealtimeUsec)
		}
		if metadata.MsgFirstRealtimeUsec != nil && metadata.MsgLastRealtimeUsec != nil {
			return
		}
	}
	reader, err := openFileWithOptions(path, readerOptions, false)
	if err != nil {
		return
	}
	defer reader.Close()
	header := reader.Header()
	if header.headEntryRealtime != 0 {
		s.addFirst(header.headEntryRealtime)
	}
	if header.tailEntryRealtime != 0 {
		s.addLast(header.tailEntryRealtime)
	}
}

func (s *netdataJournalSourceSummary) addFirst(value uint64) {
	if s.FirstRealtimeUsec == nil || value < *s.FirstRealtimeUsec {
		s.FirstRealtimeUsec = &value
	}
}

func (s *netdataJournalSourceSummary) addLast(value uint64) {
	if s.LastRealtimeUsec == nil || value > *s.LastRealtimeUsec {
		s.LastRealtimeUsec = &value
	}
}

func (s netdataJournalSourceSummary) info() string {
	coverage := "0s"
	if s.FirstRealtimeUsec != nil && s.LastRealtimeUsec != nil && *s.LastRealtimeUsec >= *s.FirstRealtimeUsec {
		coverage = humanDurationSeconds((*s.LastRealtimeUsec - *s.FirstRealtimeUsec) / 1_000_000)
	}
	lastEntry := "unknown"
	if s.LastRealtimeUsec != nil {
		lastEntry = formatRealtimeUsec(*s.LastRealtimeUsec, false)
	}
	return fmt.Sprintf("%d files, total size %s, covering %s, last entry at %s", s.Files, humanBinarySize(s.TotalSize), coverage, lastEntry)
}

func pushSourceOption(target *[]any, id string, summary netdataJournalSourceSummary) {
	if summary.Files == 0 {
		return
	}
	*target = append(*target, map[string]any{
		"id":   id,
		"name": id,
		"info": summary.info(),
		"pill": humanBinarySize(summary.TotalSize),
	})
}

func collectBootFirstRealtime(paths []string, readerOptions ReaderOptions, neededBootIDs map[string]struct{}) map[string]uint64 {
	out := make(map[string]uint64)
	if len(neededBootIDs) == 0 {
		return out
	}
	bootField := []byte("_BOOT_ID")
	for _, path := range paths {
		reader, err := openFileWithOptions(path, readerOptions, false)
		if err != nil {
			continue
		}
		_ = reader.visitFieldDataObjects(bootField, func(_ uint64, header dataHeader, payload []byte) error {
			field, bootID, ok := splitRawPayload(payload)
			if !ok || !bytes.Equal(field, bootField) {
				return nil
			}
			key := string(bootID)
			if _, ok := neededBootIDs[key]; !ok {
				return nil
			}
			entryOffset, ok, err := reader.firstDataEntryOffset(header)
			if err != nil || !ok {
				return err
			}
			entryHeader, err := reader.readEntryHeaderAt(entryOffset)
			if err != nil {
				return err
			}
			if existing, ok := out[key]; !ok || entryHeader.realtime < existing {
				out[key] = entryHeader.realtime
			}
			return nil
		})
		_ = reader.Close()
	}
	return out
}

func responseBootIDs(columnOrder []string, rows []netdataLocatedRow, facets map[string]map[string]uint64, histogram *ExplorerHistogram) map[string]struct{} {
	bootIDs := make(map[string]struct{})
	rowNeedsBootID := false
	for _, field := range columnOrder {
		if field == "_BOOT_ID" {
			rowNeedsBootID = true
			break
		}
	}
	if rowNeedsBootID {
		for _, row := range rows {
			if values, ok := rowFields(row)["_BOOT_ID"]; ok {
				for _, value := range values {
					bootIDs[string(value)] = struct{}{}
				}
			}
		}
	}
	for value := range facets["_BOOT_ID"] {
		if value != "" && value != "-" {
			bootIDs[value] = struct{}{}
		}
	}
	if histogram != nil && string(histogram.Field) == "_BOOT_ID" {
		for _, bucket := range histogram.Buckets {
			for value := range bucket.Values {
				if value != "" && value != "-" {
					bootIDs[value] = struct{}{}
				}
			}
		}
	}
	return bootIDs
}

func rowFields(row netdataLocatedRow) map[string][][]byte {
	fields := make(map[string][][]byte)
	for _, payload := range row.Row.Payloads {
		field, value, ok := splitRawPayload(payload)
		if !ok {
			continue
		}
		fields[string(field)] = append(fields[string(field)], cloneBytes(value))
	}
	fields["ND_JOURNAL_FILE"] = [][]byte{[]byte(row.FilePath)}
	if _, ok := fields["ND_JOURNAL_PROCESS"]; !ok {
		process := dynamicProcessName(fields)
		if process != "" {
			fields["ND_JOURNAL_PROCESS"] = [][]byte{[]byte(process)}
		}
	}
	return fields
}

func dynamicProcessName(fields map[string][][]byte) string {
	base := ""
	for _, field := range []string{"CONTAINER_NAME", "SYSLOG_IDENTIFIER", "_COMM"} {
		if value, ok := firstFieldValue(fields, field); ok {
			base = string(value)
			break
		}
	}
	if base == "" {
		return "-"
	}
	if pid, ok := firstFieldValue(fields, "_PID"); ok && len(pid) != 0 {
		return fmt.Sprintf("%s[%s]", base, pid)
	}
	return base
}

func makeRowTimestampsUnique(rows []netdataLocatedRow, direction Direction) {
	var lastFrom uint64
	var lastTo uint64
	initialized := false
	for i := range rows {
		timestamp := rows[i].Row.RealtimeUsec
		if initialized && timestamp >= lastFrom && timestamp <= lastTo {
			switch direction {
			case DirectionBackward:
				lastFrom = saturatingSub(lastFrom, 1)
				rows[i].Row.RealtimeUsec = lastFrom
			default:
				lastTo = saturatingAdd(lastTo, 1)
				rows[i].Row.RealtimeUsec = lastTo
			}
			continue
		}
		lastFrom = timestamp
		lastTo = timestamp
		initialized = true
	}
}

func firstFieldValue(fields map[string][][]byte, field string) ([]byte, bool) {
	values := fields[field]
	if len(values) == 0 {
		return nil, false
	}
	return values[0], true
}

func columnMetadata(key string, index int) map[string]any {
	visible := false
	filter := "none"
	fullWidth := false
	switch {
	case key == "timestamp":
		visible = true
		filter = "range"
	case key == "rowOptions":
	case key == "_HOSTNAME":
		visible = true
		filter = "facet"
	case key == "ND_JOURNAL_PROCESS" || key == "MESSAGE":
		visible = true
		fullWidth = key == "MESSAGE"
	case key == "ND_JOURNAL_FILE" || key == "_SOURCE_REALTIME_TIMESTAMP":
	case systemdColumnIsFacet(key):
		filter = "facet"
	}
	columnType := "string"
	if key == "timestamp" {
		columnType = "timestamp"
	} else if key == "rowOptions" {
		columnType = "none"
	}
	visualization := "value"
	if key == "rowOptions" {
		visualization = "rowOptions"
	}
	defaultValue := any("-")
	if key == "timestamp" || key == "rowOptions" {
		defaultValue = nil
	}
	metadata := map[string]any{
		"index":                   index,
		"unique_key":              key == "timestamp",
		"name":                    mapTimestampName(key),
		"visible":                 visible,
		"type":                    columnType,
		"visualization":           visualization,
		"value_options":           map[string]any{"transform": mapTimestampTransform(key), "decimal_points": 0, "default_value": defaultValue},
		"sort":                    "ascending",
		"sortable":                false,
		"sticky":                  false,
		"summary":                 "count",
		"filter":                  filter,
		"full_width":              fullWidth,
		"wrap":                    key != "rowOptions",
		"default_expanded_filter": key == "PRIORITY" || key == "SYSLOG_FACILITY" || key == "MESSAGE_ID",
	}
	if key == "rowOptions" {
		metadata["dummy"] = true
	}
	return metadata
}

func mapTimestampName(key string) string {
	if key == "timestamp" {
		return "Timestamp"
	}
	return key
}

func mapTimestampTransform(key string) string {
	if key == "timestamp" {
		return "datetime_usec"
	}
	return "none"
}

func systemdColumnIsFacet(key string) bool {
	if key == "MESSAGE_ID" {
		return true
	}
	return !strings.Contains(key, "MESSAGE") && !strings.Contains(key, "TIMESTAMP") && !strings.HasPrefix(key, "__")
}

func sortFacetOptions(field string, options []map[string]any) {
	sort.Slice(options, func(i, j int) bool {
		leftID, _ := options[i]["id"].(string)
		rightID, _ := options[j]["id"].(string)
		if field == "PRIORITY" {
			return parsePriority(leftID) < parsePriority(rightID)
		}
		leftCount := anyToUint64(options[i]["count"])
		rightCount := anyToUint64(options[j]["count"])
		if leftCount != rightCount {
			return leftCount > rightCount
		}
		return leftID < rightID
	})
}

func parseFTSQueryPatterns(query string) ([]ExplorerFtsPattern, [][]byte, [][]byte) {
	var terms []ExplorerFtsPattern
	var positives [][]byte
	var negatives [][]byte
	index := 0
	bytesQuery := []byte(query)
	for {
		pattern, negative, ok := nextFTSPattern(bytesQuery, &index)
		if !ok {
			break
		}
		terms = append(terms, NewExplorerFtsPattern(pattern, negative))
		if negative {
			negatives = append(negatives, pattern)
		} else {
			positives = append(positives, pattern)
		}
	}
	return terms, positives, negatives
}

func nextFTSPattern(input []byte, index *int) ([]byte, bool, bool) {
	for *index < len(input) {
		for *index < len(input) && input[*index] == '|' {
			*index++
		}
		negative := false
		if *index < len(input) && input[*index] == '!' {
			negative = true
			*index++
		}
		pattern := readFTSPattern(input, index)
		if len(pattern) != 0 {
			return pattern, negative, true
		}
	}
	return nil, false, false
}

func readFTSPattern(input []byte, index *int) []byte {
	var pattern []byte
	escaped := false
	for *index < len(input) {
		b := input[*index]
		*index++
		if b == '\\' && !escaped {
			escaped = true
			continue
		}
		if b == '|' && !escaped {
			break
		}
		pattern = append(pattern, b)
		escaped = false
	}
	return pattern
}

type netdataSourceSelection struct {
	SourceType   uint64
	ExactSources []string
}

func parseSourceSelection(value any) netdataSourceSelection {
	selection := netdataSourceSelection{SourceType: netdataSourceTypeAll}
	selections, ok := value.(map[string]any)
	if !ok {
		return selection
	}
	values, ok := parseStringArray(selections["__logs_sources"])
	if !ok {
		return selection
	}
	selection.SourceType = 0
	for _, value := range values {
		if sourceType, ok := sourceTypeForName(value); ok {
			selection.SourceType |= sourceType
		} else {
			selection.ExactSources = append(selection.ExactSources, value)
		}
	}
	return selection
}

func sourceTypeForName(value string) (uint64, bool) {
	switch value {
	case "all":
		return netdataSourceTypeAll, true
	case "all-local-logs":
		return netdataSourceTypeLocalAll, true
	case "all-remote-systems":
		return netdataSourceTypeRemoteAll, true
	case "all-local-system-logs":
		return netdataSourceTypeLocalSystem, true
	case "all-local-user-logs":
		return netdataSourceTypeLocalUser, true
	case "all-local-namespaces":
		return netdataSourceTypeLocalNamespace, true
	case "all-uncategorized":
		return netdataSourceTypeLocalOther, true
	default:
		return 0, false
	}
}

func journalFileSourceType(path string) uint64 {
	name := filepath.Base(path)
	text := filepath.ToSlash(path)
	if strings.Contains(text, "/remote/") {
		return netdataSourceTypeAll | netdataSourceTypeRemoteAll
	}
	if localNamespaceSourceName(path) != "" {
		return netdataSourceTypeAll | netdataSourceTypeLocalAll | netdataSourceTypeLocalNamespace
	}
	if strings.HasPrefix(name, "system") {
		return netdataSourceTypeAll | netdataSourceTypeLocalAll | netdataSourceTypeLocalSystem
	}
	if strings.HasPrefix(name, "user") {
		return netdataSourceTypeAll | netdataSourceTypeLocalAll | netdataSourceTypeLocalUser
	}
	return netdataSourceTypeAll | netdataSourceTypeLocalAll | netdataSourceTypeLocalOther
}

func localNamespaceSourceName(path string) string {
	parent := filepath.Base(filepath.Dir(path))
	index := strings.LastIndex(parent, ".")
	if index < 0 || index+1 >= len(parent) {
		return ""
	}
	return "namespace-" + parent[index+1:]
}

func journalFileExactSourceName(path string) string {
	text := filepath.ToSlash(path)
	if strings.Contains(text, "/remote/") {
		name := filepath.Base(path)
		if before, _, ok := strings.Cut(name, "@"); ok {
			name = before
		} else {
			name = trimJournalSuffix(name)
		}
		if strings.HasPrefix(name, "remote-") {
			return name
		}
		return ""
	}
	return localNamespaceSourceName(path)
}

func trimJournalSuffix(name string) string {
	for _, suffix := range []string{".journal~.zst", ".journal.zst", ".journal~", ".journal"} {
		if strings.HasSuffix(name, suffix) {
			return strings.TrimSuffix(name, suffix)
		}
	}
	return name
}

func parseNetdataFilters(value any) []ExplorerFilter {
	selections, ok := value.(map[string]any)
	if !ok {
		return nil
	}
	var filters []ExplorerFilter
	keys := make([]string, 0, len(selections))
	for field := range selections {
		keys = append(keys, field)
	}
	sort.Strings(keys)
	for _, field := range keys {
		if field == "query" || field == "source" || field == "__logs_sources" {
			continue
		}
		values, ok := parseStringArray(selections[field])
		if !ok {
			continue
		}
		filter := ExplorerFilter{Field: []byte(field)}
		for _, value := range values {
			filter.Values = append(filter.Values, normalizeFilterValue(field, value))
		}
		filters = append(filters, filter)
	}
	return filters
}

func normalizeFilterValue(field, value string) []byte {
	if field == "PRIORITY" {
		if priority := priorityNameToNumber(value); priority != "" {
			return []byte(priority)
		}
	}
	return []byte(value)
}

func parseStringArray(value any) ([]string, bool) {
	items, ok := value.([]any)
	if !ok {
		return nil, false
	}
	out := make([]string, 0, len(items))
	for _, item := range items {
		if value, ok := item.(string); ok {
			out = append(out, value)
		}
	}
	return out, true
}

func requestDirection(object map[string]any) Direction {
	switch getJSONString(object, "direction") {
	case "forward", "forwards", "next":
		return DirectionForward
	default:
		return DirectionBackward
	}
}

func requestAnchorAndDirection(object map[string]any, tail bool, direction Direction, after, before *uint64) (ExplorerAnchor, Direction) {
	anchor := DefaultExplorerAnchor()
	if value, ok := getJSONUint64OK(object, "anchor"); ok {
		anchor = RealtimeExplorerAnchor(normalizeTimestampToUsec(value))
	}
	if tail && anchor.Kind == ExplorerAnchorRealtime {
		return anchor, DirectionBackward
	}
	if anchorOutsideWindow(anchor, after, before) {
		return DefaultExplorerAnchor(), DirectionBackward
	}
	return anchor, direction
}

func anchorOutsideWindow(anchor ExplorerAnchor, after, before *uint64) bool {
	if anchor.Kind != ExplorerAnchorRealtime {
		return false
	}
	return (after != nil && anchor.RealtimeUsec < *after) || (before != nil && anchor.RealtimeUsec > *before)
}

func requestLimit(object map[string]any) int {
	if value, ok := getJSONUint64OK(object, "last"); ok && value != 0 {
		if value > uint64(math.MaxInt) {
			return math.MaxInt
		}
		return int(value)
	}
	return defaultNetdataItemsToReturn
}

func requestFacets(requested []string, hasRequested bool, config NetdataFunctionConfig) [][]byte {
	source := requested
	if !hasRequested {
		source = config.DefaultFacets
	}
	out := make([][]byte, 0, len(source))
	for _, field := range source {
		out = append(out, []byte(field))
	}
	return out
}

func requestHistogram(object map[string]any) string {
	value := getJSONString(object, "histogram")
	if value == "" {
		return ""
	}
	return value
}

func getJSONBool(object map[string]any, key string, fallback bool) bool {
	if value, ok := object[key].(bool); ok {
		return value
	}
	return fallback
}

func getJSONInt64(object map[string]any, key string) (int64, bool) {
	return anyToInt64OK(object[key])
}

func getJSONUint64(object map[string]any, key string, fallback uint64) uint64 {
	if value, ok := getJSONUint64OK(object, key); ok {
		return value
	}
	return fallback
}

func getJSONUint64OK(object map[string]any, key string) (uint64, bool) {
	return anyToUint64OK(object[key])
}

func getJSONString(object map[string]any, key string) string {
	if value, ok := object[key].(string); ok {
		return value
	}
	return ""
}

func optionalI64(value int64, ok bool) *int64 {
	if !ok {
		return nil
	}
	return &value
}

func normalizeNetdataTimeWindow(nowSeconds int64, afterInput, beforeInput *int64) (*uint64, *uint64) {
	after := int64(0)
	before := int64(0)
	if afterInput != nil {
		after = *afterInput
	}
	if beforeInput != nil {
		before = *beforeInput
	}
	if after == 0 && before == 0 {
		before = nowSeconds
		after = before - defaultNetdataTimeWindowSeconds
	} else {
		after, before = relativeWindowToAbsolute(nowSeconds, after, before)
	}
	if after > before {
		after, before = before, after
	}
	if after == before {
		after = before - defaultNetdataTimeWindowSeconds
	}
	if after < 0 {
		after = 0
	}
	if before < 0 {
		before = 0
	}
	afterUsec := normalizeTimestampToUsecWithRounding(uint64(after), false)
	beforeUsec := normalizeTimestampToUsecWithRounding(uint64(before), true)
	return &afterUsec, &beforeUsec
}

func relativeWindowToAbsolute(nowSeconds, after, before int64) (int64, int64) {
	if absI64(before) <= netdataRelativeTimeMaxSeconds {
		if before > 0 {
			before = -before
		}
		before = saturatingAddI64(nowSeconds, before)
	}
	if absI64(after) <= netdataRelativeTimeMaxSeconds {
		if after > 0 {
			after = -after
		}
		if after == 0 {
			after = -netdataMissingAfterRelativeSeconds
		}
		after = saturatingAddI64(saturatingAddI64(before, after), 1)
	}
	if after > before {
		after, before = before, after
	}
	if before > nowSeconds {
		delta := before - nowSeconds
		before -= delta
		after -= delta
	}
	return after, before
}

type netdataRequestEchoInput struct {
	Info                bool
	AfterRealtimeUsec   *uint64
	BeforeRealtimeUsec  *uint64
	IfModifiedSinceUsec uint64
	Anchor              ExplorerAnchor
	Direction           Direction
	Limit               int
	DataOnly            bool
	Delta               bool
	Tail                bool
	Sampling            uint64
	SourceType          uint64
	RequestedFacets     []string
	HasRequestedFacets  bool
	Selections          any
	Histogram           string
	Query               string
}

func normalizedRequestEcho(input netdataRequestEchoInput) map[string]any {
	anchorUsec := uint64(0)
	if input.Anchor.Kind == ExplorerAnchorRealtime {
		anchorUsec = input.Anchor.RealtimeUsec
	}
	out := map[string]any{
		"info":              input.Info,
		"slice":             true,
		"data_only":         input.DataOnly,
		"delta":             input.Delta,
		"tail":              input.Tail,
		"sampling":          input.Sampling,
		"source_type":       input.SourceType,
		"after":             optionalUsecToSeconds(input.AfterRealtimeUsec),
		"before":            optionalUsecToSeconds(input.BeforeRealtimeUsec),
		"if_modified_since": input.IfModifiedSinceUsec,
		"anchor":            anchorUsec,
		"direction":         netdataDirectionString(input.Direction),
		"last":              input.Limit,
		"query":             nil,
		"histogram":         nil,
	}
	if input.Query != "" {
		out["query"] = input.Query
	}
	if input.Histogram != "" {
		out["histogram"] = input.Histogram
	}
	if input.HasRequestedFacets {
		facets := make([]any, 0, len(input.RequestedFacets))
		for _, facet := range input.RequestedFacets {
			facets = append(facets, facet)
		}
		out["facets"] = facets
	}
	if selections, ok := input.Selections.(map[string]any); ok {
		clone := make(map[string]any, len(selections))
		for key, value := range selections {
			if key == "__logs_sources" {
				if sources, ok := value.([]any); ok {
					replaced := make([]any, len(sources))
					for i := range replaced {
						replaced[i] = nil
					}
					clone[key] = replaced
					continue
				}
			}
			clone[key] = value
		}
		out["selections"] = clone
	}
	return out
}

func optionalUsecToSeconds(value *uint64) uint64 {
	if value == nil {
		return 0
	}
	return *value / 1_000_000
}

func netdataDirectionString(direction Direction) string {
	if direction == DirectionForward {
		return "forward"
	}
	return "backward"
}

func normalizeTimestampToUsec(value uint64) uint64 {
	return normalizeTimestampToUsecWithRounding(value, false)
}

func normalizeTimestampToUsecWithRounding(value uint64, endOfSecond bool) uint64 {
	if value >= 1_000_000_000_000 {
		return value
	}
	if endOfSecond {
		return saturatingAdd(saturatingMul(value, 1_000_000), 999_999)
	}
	return saturatingMul(value, 1_000_000)
}

func unixNowSeconds() int64 {
	return time.Now().Unix()
}

func humanBinarySize(bytes uint64) string {
	units := []string{"B", "KiB", "MiB", "GiB", "TiB"}
	value := float64(bytes)
	unit := 0
	for value >= 1024 && unit+1 < len(units) {
		value /= 1024
		unit++
	}
	if unit == 0 {
		return fmt.Sprintf("%d%s", bytes, units[unit])
	}
	if value == math.Trunc(value) {
		return fmt.Sprintf("%.0f%s", value, units[unit])
	}
	formatted := strings.TrimRight(strings.TrimRight(fmt.Sprintf("%.2f", value), "0"), ".")
	return formatted + units[unit]
}

func humanDurationSeconds(seconds uint64) string {
	years := seconds / (365 * 86_400)
	seconds %= 365 * 86_400
	months := seconds / (30 * 86_400)
	seconds %= 30 * 86_400
	days := seconds / 86_400
	seconds %= 86_400
	hours := seconds / 3600
	minutes := (seconds % 3600) / 60
	seconds %= 60
	var parts []string
	if years != 0 {
		parts = append(parts, fmt.Sprintf("%dy", years))
	}
	if months != 0 {
		parts = append(parts, fmt.Sprintf("%dmo", months))
	}
	if days != 0 {
		parts = append(parts, fmt.Sprintf("%dd", days))
	}
	if hours != 0 {
		parts = append(parts, fmt.Sprintf("%dh", hours))
	}
	if minutes != 0 {
		parts = append(parts, fmt.Sprintf("%dm", minutes))
	}
	if seconds != 0 || len(parts) == 0 {
		parts = append(parts, fmt.Sprintf("%ds", seconds))
	}
	return strings.Join(parts, " ")
}

func fileModifiedUsec(path string) uint64 {
	info, err := os.Stat(path)
	if err != nil {
		return 0
	}
	modified := info.ModTime()
	if modified.Before(time.Unix(0, 0)) {
		return 0
	}
	return uint64(modified.UnixNano() / 1000)
}

func pushUniqueMany(target *[]string, values []string) {
	for _, value := range values {
		pushUnique(target, value)
	}
}

func pushUnique(target *[]string, value string) {
	for _, existing := range *target {
		if existing == value {
			return
		}
	}
	*target = append(*target, value)
}

func stringFields(fields [][]byte) []string {
	out := make([]string, 0, len(fields))
	for _, field := range fields {
		out = append(out, string(field))
	}
	return out
}

func stringsToBytes(fields []string) [][]byte {
	out := make([][]byte, 0, len(fields))
	for _, field := range fields {
		out = append(out, []byte(field))
	}
	return out
}

func netdataReorderKey(value string) string {
	return strings.ToLower(strings.TrimLeftFunc(value, func(r rune) bool {
		return r < '0' || (r > '9' && r < 'A') || (r > 'Z' && r < 'a') || r > 'z'
	}))
}

func histogramUpdateEverySeconds(histogram *ExplorerHistogram) uint64 {
	if histogram == nil || len(histogram.Buckets) == 0 {
		return 1
	}
	first := histogram.Buckets[0]
	seconds := first.EndRealtimeUsec - first.StartRealtimeUsec
	seconds /= 1_000_000
	if seconds == 0 {
		return 1
	}
	return seconds
}

func formatRealtimeUsec(timestamp uint64, micros bool) string {
	seconds := int64(timestamp / 1_000_000)
	usec := int64(timestamp % 1_000_000)
	t := time.Unix(seconds, usec*1000).UTC()
	if micros {
		return t.Format("2006-01-02T15:04:05.000000Z")
	}
	return t.Format("2006-01-02T15:04:05Z")
}

func priorityName(raw string) string {
	switch parsePriority(raw) {
	case 0:
		return "panic"
	case 1:
		return "alert"
	case 2:
		return "critical"
	case 3:
		return "error"
	case 4:
		return "warning"
	case 5:
		return "notice"
	case 6:
		return "info"
	case 7:
		return "debug"
	default:
		return raw
	}
}

func priorityNameToNumber(value string) string {
	switch value {
	case "panic", "emergency", "emerg":
		return "0"
	case "alert":
		return "1"
	case "critical", "crit":
		return "2"
	case "error", "err":
		return "3"
	case "warning", "warn":
		return "4"
	case "notice":
		return "5"
	case "info":
		return "6"
	case "debug":
		return "7"
	default:
		return ""
	}
}

func parsePriority(raw string) uint8 {
	value, err := strconv.ParseUint(raw, 10, 8)
	if err != nil {
		return math.MaxUint8
	}
	return uint8(value)
}

func priorityToRowSeverity(raw []byte) string {
	switch priority := parsePriority(string(raw)); {
	case priority <= 3:
		return "critical"
	case priority == 4:
		return "warning"
	case priority == 5:
		return "notice"
	case priority >= 7 && priority != math.MaxUint8:
		return "debug"
	default:
		return "normal"
	}
}

func syslogFacilityName(raw string) string {
	switch raw {
	case "0":
		return "kern"
	case "1":
		return "user"
	case "2":
		return "mail"
	case "3":
		return "daemon"
	case "4":
		return "auth"
	case "5":
		return "syslog"
	case "6":
		return "lpr"
	case "7":
		return "news"
	case "8":
		return "uucp"
	case "9":
		return "cron"
	case "10":
		return "authpriv"
	case "11":
		return "ftp"
	case "16":
		return "local0"
	case "17":
		return "local1"
	case "18":
		return "local2"
	case "19":
		return "local3"
	case "20":
		return "local4"
	case "21":
		return "local5"
	case "22":
		return "local6"
	case "23":
		return "local7"
	default:
		return raw
	}
}

func systemdFieldDisplayValue(context *DisplayContext, scope DisplayScope, field string, value []byte, resolveUserGroupNames bool) any {
	raw := string(value)
	switch field {
	case "PRIORITY":
		return priorityName(raw)
	case "SYSLOG_FACILITY":
		return syslogFacilityName(raw)
	case "ERRNO":
		return errnoName(raw)
	case "MESSAGE_ID":
		if name := messageIDName(raw); name != "" {
			if scope == DisplayScopeData {
				return fmt.Sprintf("%s (%s)", raw, name)
			}
			return name
		}
		return raw
	case "_BOOT_ID":
		if context != nil {
			if timestamp, ok := context.BootFirstRealtime[raw]; ok {
				formatted := formatRealtimeUsec(timestamp, false)
				if scope == DisplayScopeData {
					return fmt.Sprintf("%s (%s)  ", raw, formatted)
				}
				return formatted
			}
		}
		return raw
	case "_UID", "_SYSTEMD_OWNER_UID", "OBJECT_SYSTEMD_OWNER_UID", "OBJECT_UID", "_AUDIT_LOGINUID", "OBJECT_AUDIT_LOGINUID":
		if resolveUserGroupNames {
			return cachedUIDDisplay(context, raw)
		}
		return raw
	case "_GID", "OBJECT_GID":
		if resolveUserGroupNames {
			return cachedGIDDisplay(context, raw)
		}
		return raw
	case "_CAP_EFFECTIVE":
		return capEffectiveDisplay(raw)
	case "_SOURCE_REALTIME_TIMESTAMP":
		if timestamp, err := strconv.ParseUint(raw, 10, 64); err == nil && timestamp != 0 {
			return fmt.Sprintf("%s (%s)", raw, formatRealtimeUsec(timestamp, true))
		}
		return raw
	default:
		return raw
	}
}

func displayValueString(value any) string {
	switch typed := value.(type) {
	case string:
		return typed
	default:
		encoded, err := json.Marshal(typed)
		if err != nil {
			return fmt.Sprint(typed)
		}
		return string(encoded)
	}
}

func netdataRowOptions(fields map[string][][]byte) any {
	if priority, ok := firstFieldValue(fields, "PRIORITY"); ok {
		return map[string]any{"severity": priorityToRowSeverity(priority)}
	}
	return map[string]any{"severity": "normal"}
}

var netdataErrnoNames = map[uint64]string{
	1:   "EPERM",
	2:   "ENOENT",
	3:   "ESRCH",
	4:   "EINTR",
	5:   "EIO",
	6:   "ENXIO",
	7:   "E2BIG",
	8:   "ENOEXEC",
	9:   "EBADF",
	10:  "ECHILD",
	11:  "EAGAIN",
	12:  "ENOMEM",
	13:  "EACCES",
	14:  "EFAULT",
	15:  "ENOTBLK",
	16:  "EBUSY",
	17:  "EEXIST",
	18:  "EXDEV",
	19:  "ENODEV",
	20:  "ENOTDIR",
	21:  "EISDIR",
	22:  "EINVAL",
	23:  "ENFILE",
	24:  "EMFILE",
	25:  "ENOTTY",
	26:  "ETXTBSY",
	27:  "EFBIG",
	28:  "ENOSPC",
	29:  "ESPIPE",
	30:  "EROFS",
	31:  "EMLINK",
	32:  "EPIPE",
	33:  "EDOM",
	34:  "ERANGE",
	35:  "EDEADLK",
	36:  "ENAMETOOLONG",
	37:  "ENOLCK",
	38:  "ENOSYS",
	39:  "ENOTEMPTY",
	40:  "ELOOP",
	42:  "ENOMSG",
	43:  "EIDRM",
	44:  "ECHRNG",
	45:  "EL2NSYNC",
	46:  "EL3HLT",
	47:  "EL3RST",
	48:  "ELNRNG",
	49:  "EUNATCH",
	50:  "ENOCSI",
	51:  "EL2HLT",
	52:  "EBADE",
	53:  "EBADR",
	54:  "EXFULL",
	55:  "ENOANO",
	56:  "EBADRQC",
	57:  "EBADSLT",
	59:  "EBFONT",
	60:  "ENOSTR",
	61:  "ENODATA",
	62:  "ETIME",
	63:  "ENOSR",
	64:  "ENONET",
	65:  "ENOPKG",
	66:  "EREMOTE",
	67:  "ENOLINK",
	68:  "EADV",
	69:  "ESRMNT",
	70:  "ECOMM",
	71:  "EPROTO",
	72:  "EMULTIHOP",
	73:  "EDOTDOT",
	74:  "EBADMSG",
	75:  "EOVERFLOW",
	76:  "ENOTUNIQ",
	77:  "EBADFD",
	78:  "EREMCHG",
	79:  "ELIBACC",
	80:  "ELIBBAD",
	81:  "ELIBSCN",
	82:  "ELIBMAX",
	83:  "ELIBEXEC",
	84:  "EILSEQ",
	85:  "ERESTART",
	86:  "ESTRPIPE",
	87:  "EUSERS",
	88:  "ENOTSOCK",
	89:  "EDESTADDRREQ",
	90:  "EMSGSIZE",
	91:  "EPROTOTYPE",
	92:  "ENOPROTOOPT",
	93:  "EPROTONOSUPPORT",
	94:  "ESOCKTNOSUPPORT",
	95:  "ENOTSUP",
	96:  "EPFNOSUPPORT",
	97:  "EAFNOSUPPORT",
	98:  "EADDRINUSE",
	99:  "EADDRNOTAVAIL",
	100: "ENETDOWN",
	101: "ENETUNREACH",
	102: "ENETRESET",
	103: "ECONNABORTED",
	104: "ECONNRESET",
	105: "ENOBUFS",
	106: "EISCONN",
	107: "ENOTCONN",
	108: "ESHUTDOWN",
	109: "ETOOMANYREFS",
	110: "ETIMEDOUT",
	111: "ECONNREFUSED",
	112: "EHOSTDOWN",
	113: "EHOSTUNREACH",
	114: "EALREADY",
	115: "EINPROGRESS",
	116: "ESTALE",
	117: "EUCLEAN",
	118: "ENOTNAM",
	119: "ENAVAIL",
	120: "EISNAM",
	121: "EREMOTEIO",
	122: "EDQUOT",
	123: "ENOMEDIUM",
	124: "EMEDIUMTYPE",
	125: "ECANCELED",
	126: "ENOKEY",
	127: "EKEYEXPIRED",
	128: "EKEYREVOKED",
	129: "EKEYREJECTED",
	130: "EOWNERDEAD",
	131: "ENOTRECOVERABLE",
	132: "ERFKILL",
	133: "EHWPOISON",
}

func errnoName(raw string) string {
	value, err := strconv.ParseUint(raw, 10, 32)
	if err != nil {
		return raw
	}
	if name, ok := netdataErrnoNames[value]; ok {
		return fmt.Sprintf("%d (%s)", value, name)
	}
	return raw
}

func capEffectiveDisplay(raw string) string {
	if raw == "" || raw[0] < '0' || raw[0] > '9' {
		return raw
	}
	value, err := strconv.ParseUint(raw, 16, 64)
	if err != nil || value == 0 {
		return raw
	}
	capabilities := []string{
		"CHOWN", "DAC_OVERRIDE", "DAC_READ_SEARCH", "FOWNER", "FSETID", "KILL", "SETGID", "SETUID",
		"SETPCAP", "LINUX_IMMUTABLE", "NET_BIND_SERVICE", "NET_BROADCAST", "NET_ADMIN", "NET_RAW",
		"IPC_LOCK", "IPC_OWNER", "SYS_MODULE", "SYS_RAWIO", "SYS_CHROOT", "SYS_PTRACE", "SYS_PACCT",
		"SYS_ADMIN", "SYS_BOOT", "SYS_NICE", "SYS_RESOURCE", "SYS_TIME", "SYS_TTY_CONFIG", "MKNOD",
		"LEASE", "AUDIT_WRITE", "AUDIT_CONTROL", "SETFCAP", "MAC_OVERRIDE", "MAC_ADMIN", "SYSLOG",
		"WAKE_ALARM", "BLOCK_SUSPEND", "AUDIT_READ", "PERFMON", "BPF", "CHECKPOINT_RESTORE",
	}
	var names []string
	for i, name := range capabilities {
		if value&(uint64(1)<<uint(i)) != 0 {
			names = append(names, name)
		}
	}
	if len(names) == 0 {
		return raw
	}
	return fmt.Sprintf("%s (%s)", raw, strings.Join(names, " | "))
}

func cachedUIDDisplay(context *DisplayContext, raw string) string {
	if context == nil {
		return raw
	}
	if value, ok := context.uidDisplayCache[raw]; ok {
		return value
	}
	value := resolveUserName(raw)
	if value == "" {
		value = raw
	}
	context.uidDisplayCache[raw] = value
	return value
}

func cachedGIDDisplay(context *DisplayContext, raw string) string {
	if context == nil {
		return raw
	}
	if value, ok := context.gidDisplayCache[raw]; ok {
		return value
	}
	value := resolveGroupName(raw)
	if value == "" {
		value = raw
	}
	context.gidDisplayCache[raw] = value
	return value
}

func resolveUserName(raw string) string {
	return lookupColonFile("/etc/passwd", raw, 2)
}

func resolveGroupName(raw string) string {
	return lookupColonFile("/etc/group", raw, 2)
}

func lookupColonFile(path, id string, idField int) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(data), "\n") {
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Split(line, ":")
		if len(fields) > idField && fields[idField] == id {
			return fields[0]
		}
	}
	return ""
}

var netdataMessageIDNames = map[string]string{
	"f77379a8490b408bbe5f6940505a777b": "Journal started",
	"d93fb3c9c24d451a97cea615ce59c00b": "Journal stopped",
	"a596d6fe7bfa4994828e72309e95d61e": "Journal messages suppressed",
	"e9bf28e6e834481bb6f48f548ad13606": "Journal messages missed",
	"ec387f577b844b8fa948f33cad9a75e6": "Journal disk space usage",
	"fc2e22bc6ee647b6b90729ab34a250b1": "Coredump",
	"5aadd8e954dc4b1a8c954d63fd9e1137": "Coredump truncated",
	"1f4e0a44a88649939aaea34fc6da8c95": "Backtrace",
	"8d45620c1a4348dbb17410da57c60c66": "User Session created",
	"3354939424b4456d9802ca8333ed424a": "User Session terminated",
	"fcbefc5da23d428093f97c82a9290f7b": "Seat started",
	"e7852bfe46784ed0accde04bc864c2d5": "Seat removed",
	"24d8d4452573402496068381a6312df2": "VM or container started",
	"58432bd3bace477cb514b56381b8a758": "VM or container stopped",
	"c7a787079b354eaaa9e77b371893cd27": "Time change",
	"45f82f4aef7a4bbf942ce861d1f20990": "Timezone change",
	"50876a9db00f4c40bde1a2ad381c3a1b": "System configuration issues",
	"b07a249cd024414a82dd00cd181378ff": "System start-up completed",
	"eed00a68ffd84e31882105fd973abdd1": "User start-up completed",
	"6bbd95ee977941e497c48be27c254128": "Sleep start",
	"8811e6df2a8e40f58a94cea26f8ebf14": "Sleep stop",
	"98268866d1d54a499c4e98921d93bc40": "System shutdown initiated",
	"c14aaf76ec284a5fa1f105f88dfb061c": "System factory reset initiated",
	"d9ec5e95e4b646aaaea2fd05214edbda": "Container init crashed",
	"3ed0163e868a4417ab8b9e210407a96c": "System reboot failed after crash",
	"645c735537634ae0a32b15a7c6cba7d4": "Init execution froze",
	"5addb3a06a734d3396b794bf98fb2d01": "Init crashed no coredump",
	"5c9e98de4ab94c6a9d04d0ad793bd903": "Init crashed no fork",
	"5e6f1f5e4db64a0eaee3368249d20b94": "Init crashed unknown signal",
	"83f84b35ee264f74a3896a9717af34cb": "Init crashed systemd signal",
	"3a73a98baf5b4b199929e3226c0be783": "Init crashed process signal",
	"2ed18d4f78ca47f0a9bc25271c26adb4": "Init crashed waitpid failed",
	"56b1cd96f24246c5b607666fda952356": "Init crashed coredump failed",
	"4ac7566d4d7548f4981f629a28f0f829": "Init crashed coredump",
	"38e8b1e039ad469291b18b44c553a5b7": "Crash shell failed to fork",
	"872729b47dbe473eb768ccecd477beda": "Crash shell failed to execute",
	"658a67adc1c940b3b3316e7e8628834a": "Selinux failed",
	"e6f456bd92004d9580160b2207555186": "Battery low warning",
	"267437d33fdd41099ad76221cc24a335": "Battery low powering off",
	"79e05b67bc4545d1922fe47107ee60c5": "Manager mainloop failed",
	"dbb136b10ef4457ba47a795d62f108c9": "Manager no xdgdir path",
	"ed158c2df8884fa584eead2d902c1032": "Init failed to drop capability bounding set of usermode",
	"42695b500df048298bee37159caa9f2e": "Init failed to drop capability bounding set",
	"bfc2430724ab44499735b4f94cca9295": "User manager can't disable new privileges",
	"59288af523be43a28d494e41e26e4510": "Manager failed to start default target",
	"689b4fcc97b4486ea5da92db69c9e314": "Manager failed to isolate default target",
	"5ed836f1766f4a8a9fc5da45aae23b29": "Manager failed to collect passed file descriptors",
	"6a40fbfbd2ba4b8db02fb40c9cd090d7": "Init failed to fix up environment variables",
	"0e54470984ac419689743d957a119e2e": "Manager failed to allocate",
	"d67fa9f847aa4b048a2ae33535331adb": "Manager failed to write Smack",
	"af55a6f75b544431b72649f36ff6d62c": "System shutdown critical error",
	"d18e0339efb24a068d9c1060221048c2": "Init failed to fork off valgrind",
	"7d4958e842da4a758f6c1cdc7b36dcc5": "Unit starting",
	"39f53479d3a045ac8e11786248231fbf": "Unit started",
	"be02cf6855d2428ba40df7e9d022f03d": "Unit failed",
	"de5b426a63be47a7b6ac3eaac82e2f6f": "Unit stopping",
	"9d1aaa27d60140bd96365438aad20286": "Unit stopped",
	"d34d037fff1847e6ae669a370e694725": "Unit reloading",
	"7b05ebc668384222baa8881179cfda54": "Unit reloaded",
	"5eb03494b6584870a536b337290809b3": "Unit restart scheduled",
	"ae8f7b866b0347b9af31fe1c80b127c0": "Unit resources",
	"7ad2d189f7e94e70a38c781354912448": "Unit success",
	"0e4284a0caca4bfc81c0bb6786972673": "Unit skipped",
	"d9b373ed55a64feb8242e02dbe79a49c": "Unit failure result",
	"641257651c1b4ec9a8624d7a40a9e1e7": "Process execution failed",
	"98e322203f7a4ed290d09fe03c09fe15": "Unit process exited",
	"0027229ca0644181a76c4e92458afa2e": "Syslog forward missed",
	"1dee0369c7fc4736b7099b38ecb46ee7": "Mount point is not empty",
	"d989611b15e44c9dbf31e3c81256e4ed": "Unit oomd kill",
	"fe6faa94e7774663a0da52717891d8ef": "Unit out of memory",
	"b72ea4a2881545a0b50e200e55b9b06f": "Lid opened",
	"b72ea4a2881545a0b50e200e55b9b070": "Lid closed",
	"f5f416b862074b28927a48c3ba7d51ff": "System docked",
	"51e171bd585248568110144c517cca53": "System undocked",
	"b72ea4a2881545a0b50e200e55b9b071": "Power key",
	"3e0117101eb243c1b9a50db3494ab10b": "Power key long press",
	"9fa9d2c012134ec385451ffe316f97d0": "Reboot key",
	"f1c59a58c9d943668965c337caec5975": "Reboot key long press",
	"b72ea4a2881545a0b50e200e55b9b072": "Suspend key",
	"bfdaf6d312ab4007bc1fe40a15df78e8": "Suspend key long press",
	"b72ea4a2881545a0b50e200e55b9b073": "Hibernate key",
	"167836df6f7f428e98147227b2dc8945": "Hibernate key long press",
	"c772d24e9a884cbeb9ea12625c306c01": "Invalid configuration",
	"1675d7f172174098b1108bf8c7dc8f5d": "DNSSEC validation failed",
	"4d4408cfd0d144859184d1e65d7c8a65": "DNSSEC trust anchor revoked",
	"36db2dfa5a9045e1bd4af5f93e1cf057": "DNSSEC turned off",
	"b61fdac612e94b9182285b998843061f": "Username unsafe",
	"1b3bb94037f04bbf81028e135a12d293": "Mount point path not suitable",
	"010190138f494e29a0ef6669749531aa": "Device path not suitable",
	"b480325f9c394a7b802c231e51a2752c": "Nobody user unsuitable",
	"1c0454c1bd2241e0ac6fefb4bc631433": "Systemd udev settle deprecated",
	"7c8a41f37b764941a0e1780b1be2f037": "Time initial sync",
	"7db73c8af0d94eeb822ae04323fe6ab6": "Time initial bump",
	"9e7066279dc8403da79ce4b1a69064b2": "Shutdown scheduled",
	"249f6fb9e6e2428c96f3f0875681ffa3": "Shutdown canceled",
	"3f7d5ef3e54f4302b4f0b143bb270cab": "TPM PCR Extended",
	"f9b0be465ad540d0850ad32172d57c21": "Memory Trimmed",
	"a8fa8dacdb1d443e9503b8be367a6adb": "SysV Service Found",
	"187c62eb1e7f463bb530394f52cb090f": "Portable Service attached",
	"76c5c754d628490d8ecba4c9d042112b": "Portable Service detached",
	"9cf56b8baf9546cf9478783a8de42113": "systemd-networkd sysctl changed by foreign process",
	"ad7089f928ac4f7ea00c07457d47ba8a": "SRK into TPM authorization failure",
	"b2bcbaf5edf948e093ce50bbea0e81ec": "Secure Attention Key (SAK) was pressed",
	"7fc63312330b479bb32e598d47cef1a8": "dbus activate no unit",
	"ee9799dab1e24d81b7bee7759a543e1b": "dbus activate masked unit",
	"a0fa58cafd6f4f0c8d003d16ccf9e797": "dbus broker exited",
	"c8c6cde1c488439aba371a664353d9d8": "dbus dirwatch",
	"8af3357071af4153af414daae07d38e7": "dbus dispatch stats",
	"199d4300277f495f84ba4028c984214c": "dbus no sopeergroup",
	"b209c0d9d1764ab38d13b8e00d1784d6": "dbus protocol violation",
	"6fa70fa776044fa28be7a21daf42a108": "dbus receive failed",
	"0ce0fa61d1a9433dabd67417f6b8e535": "dbus service failed open",
	"24dc708d9e6a4226a3efe2033bb744de": "dbus service invalid",
	"f15d2347662d483ea9bcd8aa1a691d28": "dbus sighup",
	"0ce153587afa4095832d233c17a88001": "Gnome SM startup succeeded",
	"10dd2dc188b54a5e98970f56499d1f73": "Gnome SM unrecoverable failure",
	"f3ea493c22934e26811cd62abe8e203a": "Gnome shell started",
	"c7b39b1e006b464599465e105b361485": "Flatpak cache",
	"75ba3deb0af041a9a46272ff85d9e73e": "Flathub pulls",
	"f02bce89a54e4efab3a94a797d26204a": "Flathub pull errors",
	"dd11929c788e48bdbb6276fb5f26b08a": "Boltd starting",
	"1e6061a9fbd44501b3ccc368119f2b69": "Netdata startup",
	"ed4cdb8f1beb4ad3b57cb3cae2d162fa": "Netdata connection from child",
	"6e2e3839067648968b646045dbf28d66": "Netdata connection to parent",
	"9ce0cb58ab8b44df82c4bf1ad9ee22de": "Netdata alert transition",
	"6db0018e83e34320ae2a659d78019fb7": "Netdata alert notification",
	"23e93dfccbf64e11aac858b9410d8a82": "Netdata fatal message",
	"8ddaf5ba33a74078b609250db1e951f3": "Sensor state transition",
	"ec87a56120d5431bace51e2fb8bba243": "Netdata log flood protection",
	"acb33cb95778476baac702eb7e4e151d": "Netdata Cloud connection",
	"d1f59606dd4d41e3b217a0cfcae8e632": "Netdata extreme cardinality",
	"02f47d350af5449197bf7a95b605a468": "Netdata exit reason",
	"4fdf40816c124623a032b7fe73beacb8": "Netdata dynamic configuration",
}

func messageIDName(raw string) string {
	return netdataMessageIDNames[raw]
}

func facetGroupIsReportable(values map[string]uint64) bool {
	for value := range values {
		if value != "" && value != "-" {
			return true
		}
	}
	return false
}

func netdataFacetValue(value string) string {
	if len(value) > netdataFacetMaxValueLength {
		return value[:netdataFacetMaxValueLength]
	}
	return value
}

func addNetdataFacetCount(target map[string]uint64, value string, count uint64) {
	target[netdataFacetValue(value)] += count
}

func cloneByteSlices(values [][]byte) [][]byte {
	out := make([][]byte, 0, len(values))
	for _, value := range values {
		out = append(out, cloneBytes(value))
	}
	return out
}

func cloneExplorerFilters(filters []ExplorerFilter) []ExplorerFilter {
	out := make([]ExplorerFilter, 0, len(filters))
	for _, filter := range filters {
		copyFilter := ExplorerFilter{Field: cloneBytes(filter.Field)}
		copyFilter.Values = cloneByteSlices(filter.Values)
		out = append(out, copyFilter)
	}
	return out
}

func cloneFTSTerms(terms []ExplorerFtsPattern) []ExplorerFtsPattern {
	out := make([]ExplorerFtsPattern, 0, len(terms))
	for _, term := range terms {
		copyTerm := ExplorerFtsPattern{Negative: term.Negative}
		copyTerm.Parts = cloneByteSlices(term.Parts)
		out = append(out, copyTerm)
	}
	return out
}

func anyToUint64(value any) uint64 {
	out, _ := anyToUint64OK(value)
	return out
}

func anyToUint64OK(value any) (uint64, bool) {
	switch typed := value.(type) {
	case uint64:
		return typed, true
	case uint:
		return uint64(typed), true
	case int:
		if typed >= 0 {
			return uint64(typed), true
		}
	case int64:
		if typed >= 0 {
			return uint64(typed), true
		}
	case float64:
		if typed >= 0 && typed <= float64(math.MaxUint64) {
			return uint64(typed), true
		}
	case json.Number:
		if value, err := strconv.ParseUint(string(typed), 10, 64); err == nil {
			return value, true
		}
	}
	return 0, false
}

func anyToInt64OK(value any) (int64, bool) {
	switch typed := value.(type) {
	case int:
		return int64(typed), true
	case int64:
		return typed, true
	case uint64:
		if typed <= math.MaxInt64 {
			return int64(typed), true
		}
	case float64:
		if typed >= float64(math.MinInt64) && typed <= float64(math.MaxInt64) {
			return int64(typed), true
		}
	case json.Number:
		if value, err := strconv.ParseInt(string(typed), 10, 64); err == nil {
			return value, true
		}
	}
	return 0, false
}

func saturatingMul(a, b uint64) uint64 {
	if a == 0 || b == 0 {
		return 0
	}
	if a > math.MaxUint64/b {
		return math.MaxUint64
	}
	return a * b
}

func saturatingAddI64(a, b int64) int64 {
	if b > 0 && a > math.MaxInt64-b {
		return math.MaxInt64
	}
	if b < 0 && a < math.MinInt64-b {
		return math.MinInt64
	}
	return a + b
}

func absI64(value int64) int64 {
	if value < 0 {
		return -value
	}
	return value
}
