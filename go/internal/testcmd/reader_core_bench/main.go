package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"math/bits"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const defaultWindowSize = 32 * 1024 * 1024

type inputsFlag []string

func (f *inputsFlag) String() string {
	return strings.Join(*f, ",")
}

func (f *inputsFlag) Set(value string) error {
	*f = append(*f, value)
	return nil
}

type counts struct {
	records  uint64
	fields   uint64
	bytes    uint64
	checksum uint64
	extra    map[string]interface{}
}

type benchConfig struct {
	inputs                    []string
	mode                      string
	surface                   string
	direction                 string
	bounds                    string
	mmapStrategy              string
	windowSize                uint64
	cpuProfile                string
	memProfile                string
	loops                     int
	options                   journal.ReaderOptions
	explorerFacets            []string
	explorerFilters           []string
	explorerHistogram         string
	explorerLimit             int
	explorerFTSPatterns       []string
	explorerFieldMode         string
	explorerUseSourceRealtime bool
	explorerStrategy          string
	explorerAfterUsec         uint64
	explorerBeforeUsec        uint64
	explorerAfterSet          bool
	explorerBeforeSet         bool
}

func (c *counts) addRun(other counts) {
	c.records += other.records
	c.fields += other.fields
	c.bytes += other.bytes
	c.checksum = bits.RotateLeft64(c.checksum, 11) ^ other.checksum
	if other.extra != nil {
		c.extra = other.extra
	}
}

func (c *counts) addPayload(payload []byte) {
	c.fields++
	c.bytes += uint64(len(payload))
	c.checksum = checksumPayload(c.checksum, payload)
}

func (c *counts) addRecordMarker(value uint64) {
	c.records++
	c.checksum = bits.RotateLeft64(c.checksum, 7) ^ value
}

func checksumPayload(checksum uint64, payload []byte) uint64 {
	checksum = bits.RotateLeft64(checksum, 5) ^ uint64(len(payload))
	if len(payload) > 0 {
		checksum ^= uint64(payload[0]) << 8
		checksum ^= uint64(payload[len(payload)-1])
	}
	return checksum
}

func checksumBytes(checksum uint64, payload []byte) uint64 {
	checksum = bits.RotateLeft64(checksum, 11) ^ uint64(len(payload))
	for index, value := range payload {
		if index >= 8 {
			break
		}
		checksum = bits.RotateLeft64(checksum, 3) ^ uint64(value)
	}
	if len(payload) > 0 {
		checksum ^= uint64(payload[len(payload)-1]) << 17
	}
	return checksum
}

func parseOptions(bounds, mmapStrategy string, windowSize uint64) (journal.ReaderOptions, error) {
	opts := journal.DefaultReaderOptions()
	switch bounds {
	case "live":
		opts = opts.WithBounds(journal.ReaderBoundsLive)
	case "snapshot":
		opts = opts.WithBounds(journal.ReaderBoundsSnapshot)
	default:
		return opts, fmt.Errorf("invalid --bounds: %s", bounds)
	}
	switch mmapStrategy {
	case "read-at", "buffer":
		opts = opts.WithAccessMode(journal.ReaderAccessReadAt)
	case "mmap":
		opts = opts.WithAccessMode(journal.ReaderAccessMmap)
	case "auto":
		opts = opts.WithAccessMode(journal.ReaderAccessAuto)
	default:
		return opts, fmt.Errorf("invalid --mmap-strategy: %s", mmapStrategy)
	}
	opts = opts.WithWindowSize(windowSize)
	return opts, nil
}

func parseExplorerFieldMode(value string) (journal.ExplorerFieldMode, error) {
	switch value {
	case "all-values":
		return journal.ExplorerFieldModeAllValues, nil
	case "first-value":
		return journal.ExplorerFieldModeFirstValue, nil
	default:
		return journal.ExplorerFieldModeFirstValue, fmt.Errorf("invalid --explorer-field-mode: %s", value)
	}
}

func parseExplorerStrategy(value string) (journal.ExplorerStrategy, error) {
	switch value {
	case "traversal":
		return journal.ExplorerStrategyTraversal, nil
	case "index":
		return journal.ExplorerStrategyIndex, nil
	case "compare":
		return journal.ExplorerStrategyCompare, nil
	default:
		return journal.ExplorerStrategyTraversal, fmt.Errorf("invalid --explorer-strategy: %s", value)
	}
}

func parseExplorerFilters(rawFilters []string) ([]journal.ExplorerFilter, error) {
	grouped := make(map[string][][]byte)
	for _, rawFilter := range rawFilters {
		field, value, ok := strings.Cut(rawFilter, "=")
		if !ok || field == "" || strings.Contains(field, "=") {
			return nil, fmt.Errorf("--explorer-filter must be FIELD=VALUE: %s", rawFilter)
		}
		grouped[field] = append(grouped[field], []byte(value))
	}
	out := make([]journal.ExplorerFilter, 0, len(grouped))
	for field, values := range grouped {
		out = append(out, journal.NewExplorerFilter([]byte(field), values...))
	}
	return out, nil
}

func explorerQueryFromConfig(cfg benchConfig) (journal.ExplorerQuery, error) {
	fieldMode, err := parseExplorerFieldMode(cfg.explorerFieldMode)
	if err != nil {
		return journal.ExplorerQuery{}, err
	}
	filters, err := parseExplorerFilters(cfg.explorerFilters)
	if err != nil {
		return journal.ExplorerQuery{}, err
	}
	query := journal.DefaultExplorerQuery()
	query.Direction = journal.DirectionForward
	if cfg.direction == "backward" {
		query.Direction = journal.DirectionBackward
	}
	query.Limit = cfg.explorerLimit
	query.Filters = filters
	query.FieldMode = fieldMode
	query.UseSourceRealtime = cfg.explorerUseSourceRealtime
	for _, field := range cfg.explorerFacets {
		query.Facets = append(query.Facets, []byte(field))
	}
	if cfg.explorerHistogram != "" {
		query.Histogram = []byte(cfg.explorerHistogram)
	}
	for _, pattern := range cfg.explorerFTSPatterns {
		query = query.WithFTSPattern([]byte(pattern))
	}
	if cfg.explorerAfterSet {
		value := cfg.explorerAfterUsec
		query.AfterRealtimeUsec = &value
	}
	if cfg.explorerBeforeSet {
		value := cfg.explorerBeforeUsec
		query.BeforeRealtimeUsec = &value
	}
	return query, nil
}

func facetSummary(facets map[string]map[string]uint64) map[string]interface{} {
	fields := make([]string, 0, len(facets))
	for field := range facets {
		fields = append(fields, field)
	}
	sort.Strings(fields)
	var valueCount, updates, checksum uint64
	for _, field := range fields {
		checksum = checksumBytes(checksum, []byte(field))
		values := make([]string, 0, len(facets[field]))
		for value := range facets[field] {
			values = append(values, value)
		}
		sort.Strings(values)
		valueCount += uint64(len(values))
		for _, value := range values {
			count := facets[field][value]
			checksum = checksumBytes(checksum, []byte(value)) ^ count
			updates += count
		}
	}
	return map[string]interface{}{
		"facet_fields":   uint64(len(fields)),
		"facet_values":   valueCount,
		"facet_updates":  updates,
		"facet_checksum": checksum,
	}
}

func histogramSummary(histogram *journal.ExplorerHistogram) map[string]interface{} {
	if histogram == nil {
		return nil
	}
	checksum := checksumBytes(0, histogram.Field)
	var updates uint64
	for _, bucket := range histogram.Buckets {
		checksum ^= bits.RotateLeft64(bucket.StartRealtimeUsec, 13)
		checksum ^= bits.RotateLeft64(bucket.EndRealtimeUsec, 17)
		values := make([]string, 0, len(bucket.Values))
		for value := range bucket.Values {
			values = append(values, value)
		}
		sort.Strings(values)
		for _, value := range values {
			count := bucket.Values[value]
			checksum = checksumBytes(checksum, []byte(value)) ^ count
			updates += count
		}
	}
	return map[string]interface{}{
		"field_checksum":     checksumBytes(0, histogram.Field),
		"buckets":            len(histogram.Buckets),
		"value_updates":      updates,
		"histogram_checksum": checksum,
	}
}

func processStatusKB() map[string]uint64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return map[string]uint64{}
	}
	wanted := map[string]struct{}{
		"VmSize": {}, "VmPeak": {}, "VmRSS": {}, "VmHWM": {}, "RssAnon": {},
		"RssFile": {}, "RssShmem": {}, "VmData": {}, "VmStk": {}, "VmExe": {},
		"VmLib": {}, "VmPTE": {},
	}
	out := make(map[string]uint64)
	for _, line := range strings.Split(string(data), "\n") {
		key, rest, ok := strings.Cut(line, ":")
		if !ok {
			continue
		}
		if _, ok := wanted[key]; !ok {
			continue
		}
		fields := strings.Fields(rest)
		if len(fields) == 0 {
			continue
		}
		value, err := strconv.ParseUint(fields[0], 10, 64)
		if err == nil {
			out[key+"_kb"] = value
		}
	}
	return out
}

type sdkReader interface {
	Close() error
	SeekHead() error
	SeekTail() error
	Step() (bool, error)
	StepBack() (bool, error)
	GetEntry() (*journal.Entry, error)
	GetRealtimeUsec() (uint64, error)
	VisitEntryPayloads(func([]byte) error) error
}

func openSDKReader(surface string, inputs []string, opts journal.ReaderOptions) (sdkReader, error) {
	switch surface {
	case "file":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("file surface requires exactly one --input")
		}
		return journal.OpenFileWithOptions(inputs[0], opts)
	case "open-files":
		return journal.OpenFilesWithOptions(inputs, opts)
	case "directory":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("directory surface requires exactly one --input")
		}
		return journal.OpenDirectoryWithOptions(inputs[0], opts)
	default:
		return nil, fmt.Errorf("invalid --surface: %s", surface)
	}
}

func seekSDKReader(reader sdkReader, direction string) error {
	if direction == "backward" {
		return reader.SeekTail()
	}
	return reader.SeekHead()
}

func stepSDKReader(reader sdkReader, direction string) (bool, error) {
	if direction == "backward" {
		return reader.StepBack()
	}
	return reader.Step()
}

func addSDKEntry(result *counts, reader sdkReader) error {
	entry, err := reader.GetEntry()
	if err != nil {
		return err
	}
	result.addRecordMarker(entry.Realtime)
	for _, payload := range entry.Payloads {
		result.addPayload(payload)
	}
	return nil
}

func addSDKPayloads(result *counts, reader sdkReader) error {
	realtime, err := reader.GetRealtimeUsec()
	if err != nil {
		return err
	}
	result.addRecordMarker(realtime)
	return reader.VisitEntryPayloads(func(payload []byte) error {
		result.addPayload(payload)
		return nil
	})
}

func readSDK(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	reader, err := openSDKReader(surface, inputs, opts)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	if err := seekSDKReader(reader, direction); err != nil {
		return counts{}, err
	}

	var result counts
	if statsReader, ok := reader.(interface {
		AccessStats() journal.ReaderAccessStats
	}); ok {
		result.extra = map[string]interface{}{"reader_access_stats": statsReader.AccessStats()}
	}
	for {
		ok, err := stepSDKReader(reader, direction)
		if err != nil {
			return counts{}, err
		}
		if !ok {
			break
		}
		switch mode {
		case "sdk-entry":
			err = addSDKEntry(&result, reader)
		case "sdk-payloads":
			err = addSDKPayloads(&result, reader)
		default:
			return counts{}, fmt.Errorf("invalid SDK mode: %s", mode)
		}
		if err != nil {
			return counts{}, err
		}
	}
	return result, nil
}

type facadeHandle interface {
	Close() error
	SeekHead() error
	SeekTail() error
	Next() (int, error)
	Previous() (int, error)
	GetRealtimeUsec() (uint64, error)
	RestartData() error
	EnumerateAvailableData() ([]byte, bool, error)
}

func openFacade(surface string, inputs []string, opts journal.ReaderOptions) (facadeHandle, error) {
	var (
		j   facadeHandle
		err error
	)
	switch surface {
	case "file", "open-files":
		j, err = journal.SdJournalOpenFilesWithOptions(inputs, 0, opts)
	case "directory":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("directory surface requires exactly one --input")
		}
		j, err = journal.SdJournalOpenDirectoryWithOptions(inputs[0], 0, opts)
	default:
		return nil, fmt.Errorf("invalid --surface: %s", surface)
	}
	return j, err
}

func seekFacade(j facadeHandle, direction string) error {
	if direction == "backward" {
		return j.SeekTail()
	}
	return j.SeekHead()
}

func stepFacade(j facadeHandle, direction string) (int, error) {
	if direction == "backward" {
		return j.Previous()
	}
	return j.Next()
}

func addFacadeNext(result *counts, j facadeHandle) error {
	realtime, err := j.GetRealtimeUsec()
	if err != nil {
		return err
	}
	result.addRecordMarker(realtime)
	return nil
}

func addFacadeData(result *counts, j facadeHandle) error {
	if err := addFacadeNext(result, j); err != nil {
		return err
	}
	if err := j.RestartData(); err != nil {
		return err
	}
	for {
		payload, ok, err := j.EnumerateAvailableData()
		if err != nil || !ok {
			return err
		}
		result.addPayload(payload)
	}
}

func readFacade(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	j, err := openFacade(surface, inputs, opts)
	if err != nil {
		return counts{}, err
	}
	defer j.Close()
	if err := seekFacade(j, direction); err != nil {
		return counts{}, err
	}

	var result counts
	for {
		n, err := stepFacade(j, direction)
		if err != nil {
			return counts{}, err
		}
		if n == 0 {
			break
		}
		switch mode {
		case "facade-next":
			err = addFacadeNext(&result, j)
		case "facade-data":
			err = addFacadeData(&result, j)
		default:
			return counts{}, fmt.Errorf("invalid facade mode: %s", mode)
		}
		if err != nil {
			return counts{}, err
		}
	}
	return result, nil
}

func readExplorerQuery(cfg benchConfig) (counts, error) {
	if cfg.surface != "file" {
		return counts{}, fmt.Errorf("explorer-query mode currently requires --surface file")
	}
	if len(cfg.inputs) != 1 {
		return counts{}, fmt.Errorf("explorer-query mode requires exactly one --input")
	}
	reader, err := journal.OpenFileWithOptions(cfg.inputs[0], cfg.options)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	query, err := explorerQueryFromConfig(cfg)
	if err != nil {
		return counts{}, err
	}
	strategy, err := parseExplorerStrategy(cfg.explorerStrategy)
	if err != nil {
		return counts{}, err
	}
	result, err := reader.ExploreWithStrategy(query, strategy)
	if err != nil {
		return counts{}, err
	}
	out := counts{
		records:  explorerLogicalRecords(result.Stats),
		fields:   result.Stats.DataRefsSeen,
		checksum: 0,
		extra:    make(map[string]interface{}),
	}
	addExplorerRowChecksums(&out, result.Rows)
	out.extra["facet_summary"] = facetSummary(result.Facets)
	if summary := histogramSummary(result.Histogram); summary != nil {
		out.extra["histogram_summary"] = summary
	}
	out.extra["explorer_stats"] = result.Stats
	addExplorerComparison(out.extra, result.Comparison)
	return out, nil
}

func explorerLogicalRecords(stats journal.ExplorerStats) uint64 {
	logicalRecords := stats.RowsExamined
	logicalRecords = maxBenchCount(logicalRecords, stats.FacetRowsMatched)
	logicalRecords = maxBenchCount(logicalRecords, stats.RowsMatched)
	logicalRecords = maxBenchCount(logicalRecords, stats.HistogramUpdates)
	return logicalRecords
}

func maxBenchCount(left, right uint64) uint64 {
	if right > left {
		return right
	}
	return left
}

func addExplorerRowChecksums(out *counts, rows []journal.ExplorerRow) {
	for _, row := range rows {
		out.checksum = bits.RotateLeft64(out.checksum, 7) ^ row.RealtimeUsec
		for _, payload := range row.Payloads {
			out.bytes += uint64(len(payload))
			out.checksum = checksumPayload(out.checksum, payload)
		}
	}
}

func addExplorerComparison(extra map[string]interface{}, comparison *journal.ExplorerComparison) {
	if comparison == nil {
		return
	}
	extra["explorer_comparison"] = map[string]interface{}{
		"traversal_duration_ns": uint64(comparison.TraversalDuration),
		"index_duration_ns":     uint64(comparison.IndexDuration),
		"traversal_stats":       comparison.TraversalStats,
		"index_stats":           comparison.IndexStats,
	}
}

func parseBenchConfig() (benchConfig, error) {
	var inputs inputsFlag
	var explorerFacets inputsFlag
	var explorerFilters inputsFlag
	var explorerFTSPatterns inputsFlag
	mode := flag.String("mode", "sdk-payloads", "")
	surface := flag.String("surface", "file", "")
	direction := flag.String("direction", "forward", "")
	bounds := flag.String("bounds", "live", "")
	mmapStrategy := flag.String("mmap-strategy", "auto", "")
	windowSize := flag.Uint64("window-size", defaultWindowSize, "")
	cpuProfile := flag.String("cpuprofile", "", "")
	memProfile := flag.String("memprofile", "", "")
	loops := flag.Int("loops", 1, "")
	explorerHistogram := flag.String("explorer-histogram", "", "")
	explorerLimit := flag.Int("explorer-limit", 0, "")
	explorerFieldMode := flag.String("explorer-field-mode", "first-value", "")
	explorerUseSourceRealtime := flag.Bool("explorer-use-source-realtime", false, "")
	explorerStrategy := flag.String("explorer-strategy", "traversal", "")
	explorerAfterUsec := flag.Uint64("explorer-after-usec", 0, "")
	explorerBeforeUsec := flag.Uint64("explorer-before-usec", 0, "")
	flag.Var(&inputs, "input", "")
	flag.Var(&explorerFacets, "explorer-facet", "")
	flag.Var(&explorerFilters, "explorer-filter", "")
	flag.Var(&explorerFTSPatterns, "explorer-fts", "")
	flag.Parse()
	if len(inputs) == 0 {
		return benchConfig{}, errors.New("missing --input")
	}
	if *direction != "forward" && *direction != "backward" {
		return benchConfig{}, fmt.Errorf("invalid --direction: %s", *direction)
	}
	if *loops < 1 {
		return benchConfig{}, fmt.Errorf("invalid --loops: %d", *loops)
	}
	opts, err := parseOptions(*bounds, *mmapStrategy, *windowSize)
	if err != nil {
		return benchConfig{}, err
	}
	return benchConfig{
		inputs:                    inputs,
		mode:                      *mode,
		surface:                   *surface,
		direction:                 *direction,
		bounds:                    *bounds,
		mmapStrategy:              *mmapStrategy,
		windowSize:                *windowSize,
		cpuProfile:                *cpuProfile,
		memProfile:                *memProfile,
		loops:                     *loops,
		options:                   opts,
		explorerFacets:            explorerFacets,
		explorerFilters:           explorerFilters,
		explorerHistogram:         *explorerHistogram,
		explorerLimit:             *explorerLimit,
		explorerFTSPatterns:       explorerFTSPatterns,
		explorerFieldMode:         *explorerFieldMode,
		explorerUseSourceRealtime: *explorerUseSourceRealtime,
		explorerStrategy:          *explorerStrategy,
		explorerAfterUsec:         *explorerAfterUsec,
		explorerBeforeUsec:        *explorerBeforeUsec,
		explorerAfterSet:          isFlagSet("explorer-after-usec"),
		explorerBeforeSet:         isFlagSet("explorer-before-usec"),
	}, nil
}

func isFlagSet(name string) bool {
	found := false
	flag.Visit(func(f *flag.Flag) {
		if f.Name == name {
			found = true
		}
	})
	return found
}

func main() {
	cfg, err := parseBenchConfig()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	before := processStatusKB()
	if cfg.cpuProfile != "" {
		f, err := os.Create(cfg.cpuProfile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "create cpu profile: %v\n", err)
			os.Exit(1)
		}
		if err := pprof.StartCPUProfile(f); err != nil {
			_ = f.Close()
			fmt.Fprintf(os.Stderr, "start cpu profile: %v\n", err)
			os.Exit(1)
		}
		defer f.Close()
		defer pprof.StopCPUProfile()
	}
	started := time.Now()
	result, err := runBenchLoops(cfg)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	seconds := time.Since(started).Seconds()
	if err := writeMemProfile(cfg.memProfile); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	after := processStatusKB()
	encoded, err := benchOutput(cfg, result, seconds, before, after)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println(string(encoded))
}

func runBenchLoops(cfg benchConfig) (counts, error) {
	var result counts
	for range cfg.loops {
		partial, err := readBenchOnce(cfg)
		if err != nil {
			return counts{}, err
		}
		result.addRun(partial)
	}
	return result, nil
}

func readBenchOnce(cfg benchConfig) (counts, error) {
	switch cfg.mode {
	case "sdk-entry", "sdk-payloads":
		return readSDK(cfg.surface, cfg.mode, cfg.direction, cfg.inputs, cfg.options)
	case "facade-next", "facade-data":
		return readFacade(cfg.surface, cfg.mode, cfg.direction, cfg.inputs, cfg.options)
	case "explorer-query":
		return readExplorerQuery(cfg)
	default:
		return counts{}, fmt.Errorf("invalid --mode: %s", cfg.mode)
	}
}

func writeMemProfile(path string) error {
	if path == "" {
		return nil
	}
	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create memory profile: %w", err)
	}
	runtime.GC()
	if err := pprof.WriteHeapProfile(f); err != nil {
		_ = f.Close()
		return fmt.Errorf("write memory profile: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("close memory profile: %w", err)
	}
	return nil
}

func benchOutput(
	cfg benchConfig,
	result counts,
	seconds float64,
	before map[string]uint64,
	after map[string]uint64,
) ([]byte, error) {
	output := map[string]interface{}{
		"language":               "go",
		"surface":                cfg.surface,
		"mode":                   cfg.mode,
		"direction":              cfg.direction,
		"records":                result.records,
		"fields":                 result.fields,
		"bytes":                  result.bytes,
		"checksum":               result.checksum,
		"read_seconds":           seconds,
		"read_rows_per_second":   float64(result.records) / seconds,
		"read_fields_per_second": float64(result.fields) / seconds,
		"read_bytes_per_second":  float64(result.bytes) / seconds,
		"inputs":                 absoluteInputs(cfg.inputs),
		"window_size":            cfg.windowSize,
		"bounds":                 cfg.bounds,
		"mmap_strategy":          cfg.mmapStrategy,
		"loops":                  cfg.loops,
		"timer_excludes":         []string{"fixture generation", "process startup", "external verification"},
		"process_status_before":  before,
		"process_status_after":   after,
		"errors":                 []string{},
		"explorer": map[string]interface{}{
			"facets":              cfg.explorerFacets,
			"filters":             cfg.explorerFilters,
			"histogram":           cfg.explorerHistogram,
			"limit":               cfg.explorerLimit,
			"fts_patterns":        cfg.explorerFTSPatterns,
			"field_mode":          cfg.explorerFieldMode,
			"use_source_realtime": cfg.explorerUseSourceRealtime,
			"strategy":            cfg.explorerStrategy,
			"after_usec":          optionalUint64(cfg.explorerAfterUsec, cfg.explorerAfterSet),
			"before_usec":         optionalUint64(cfg.explorerBeforeUsec, cfg.explorerBeforeSet),
		},
	}
	if result.extra == nil {
		output["extra"] = map[string]interface{}{}
	} else {
		output["extra"] = result.extra
	}
	return json.Marshal(output)
}

func optionalUint64(value uint64, ok bool) interface{} {
	if !ok {
		return nil
	}
	return value
}

func absoluteInputs(inputs []string) []string {
	absInputs := make([]string, 0, len(inputs))
	for _, input := range inputs {
		abs, err := filepath.Abs(input)
		if err != nil {
			absInputs = append(absInputs, input)
		} else {
			absInputs = append(absInputs, abs)
		}
	}
	return absInputs
}
