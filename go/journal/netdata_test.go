package journal

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestNetdataFunctionInfoResponse(t *testing.T) {
	fn := SystemdJournalPluginCompatibleNetdataFunction()
	response, err := fn.RunDirectoryRequestJSONWithOptions(t.TempDir(), map[string]any{"info": true}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(info) error = %v", err)
	}
	if got := response["status"]; got != 200 {
		t.Fatalf("status = %v, want 200", got)
	}
	versions, ok := response["versions"].(map[string]any)
	if !ok {
		t.Fatalf("versions = %T, want object", response["versions"])
	}
	if got := versions["sdk"]; got != "go" {
		t.Fatalf("versions.sdk = %v, want go", got)
	}
	source := requiredSourceParamForTest(t, response)
	if got := source["id"]; got != "__logs_sources" {
		t.Fatalf("source selector id = %v, want __logs_sources", got)
	}
	if got := source["name"]; got != defaultNetdataSourceSelectorName {
		t.Fatalf("source selector name = %v, want %q", got, defaultNetdataSourceSelectorName)
	}
	if got := source["help"]; got != defaultNetdataSourceSelectorHelp {
		t.Fatalf("source selector help = %v, want %q", got, defaultNetdataSourceSelectorHelp)
	}
}

func TestNetdataFunctionCustomSourceSelectorMetadata(t *testing.T) {
	config := SystemdJournalNetdataFunctionConfig()
	config.SourceSelectorName = "Trap Jobs"
	config.SourceSelectorHelp = "Select the trap job to query"
	fn := NewNetdataJournalFunction(config, SystemdJournalPluginProfile{})

	response, err := fn.RunDirectoryRequestJSONWithOptions(t.TempDir(), map[string]any{"info": true}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(info) error = %v", err)
	}
	source := requiredSourceParamForTest(t, response)
	if got := source["id"]; got != "__logs_sources" {
		t.Fatalf("source selector id = %v, want __logs_sources", got)
	}
	if got := source["name"]; got != "Trap Jobs" {
		t.Fatalf("source selector name = %v, want Trap Jobs", got)
	}
	if got := source["help"]; got != "Select the trap job to query" {
		t.Fatalf("source selector help = %v, want custom help", got)
	}
}

func TestNetdataFunctionZeroValueConfigUsesDefaultSourceSelectorMetadata(t *testing.T) {
	fn := NewNetdataJournalFunction(NetdataFunctionConfig{}, nil)
	response, err := fn.RunDirectoryRequestJSONWithOptions(t.TempDir(), map[string]any{"info": true}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(info) error = %v", err)
	}
	source := requiredSourceParamForTest(t, response)
	if got := source["name"]; got != defaultNetdataSourceSelectorName {
		t.Fatalf("source selector name = %v, want %q", got, defaultNetdataSourceSelectorName)
	}
	if got := source["help"]; got != defaultNetdataSourceSelectorHelp {
		t.Fatalf("source selector help = %v, want %q", got, defaultNetdataSourceSelectorHelp)
	}
}

func requiredSourceParamForTest(t *testing.T, response map[string]any) map[string]any {
	t.Helper()
	params, ok := response["required_params"].([]any)
	if !ok {
		t.Fatalf("required_params = %T, want array", response["required_params"])
	}
	for _, param := range params {
		source, ok := param.(map[string]any)
		if !ok {
			t.Fatalf("required param = %T, want object", param)
		}
		if source["id"] == "__logs_sources" {
			return source
		}
	}
	t.Fatalf("required_params missing __logs_sources: %#v", params)
	return nil
}

func TestNetdataCollectBootFirstRealtimeUsesBootIndex(t *testing.T) {
	bootA := UUID{0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf}
	bootB := UUID{0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf}
	path := filepath.Join(t.TempDir(), "boots.journal")
	opts := testOptions()
	opts.BootID = bootA
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	rows := []struct {
		boot     UUID
		realtime uint64
		message  string
	}{
		{boot: bootA, realtime: 100, message: "boot-a-early"},
		{boot: bootB, realtime: 200, message: "boot-b"},
		{boot: bootA, realtime: 300, message: "boot-a-late"},
	}
	for _, row := range rows {
		err := w.Append([]Field{
			StringField("MESSAGE", row.message),
			StringField("_BOOT_ID", row.boot.String()),
		}, EntryOptions{RealtimeUsec: row.realtime, RealtimeUsecSet: true, MonotonicUsec: row.realtime, MonotonicUsecSet: true, BootID: row.boot})
		if err != nil {
			t.Fatalf("Append(%s) error = %v", row.message, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	got := collectBootFirstRealtime([]string{path}, DefaultReaderOptions(), map[string]struct{}{
		bootA.String(): {},
		bootB.String(): {},
	})
	if got[bootA.String()] != 100 {
		t.Fatalf("boot A first realtime = %d, want 100", got[bootA.String()])
	}
	if got[bootB.String()] != 200 {
		t.Fatalf("boot B first realtime = %d, want 200", got[bootB.String()])
	}
}

func TestNetdataCollectBootFirstRealtimeSkipsUnneededAndKeepsZeroRealtime(t *testing.T) {
	bootA := UUID{0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf}
	bootB := UUID{0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde, 0xdf}
	path := filepath.Join(t.TempDir(), "boot-edge.journal")
	opts := testOptions()
	opts.BootID = bootA
	w, err := Create(path, opts)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	rows := []struct {
		boot     UUID
		realtime uint64
		message  string
	}{
		{boot: bootA, realtime: 0, message: "boot-a-zero"},
		{boot: bootB, realtime: 50, message: "boot-b-unneeded"},
		{boot: bootA, realtime: 300, message: "boot-a-late"},
	}
	for _, row := range rows {
		err := w.Append([]Field{
			StringField("MESSAGE", row.message),
			StringField("_BOOT_ID", row.boot.String()),
		}, EntryOptions{RealtimeUsec: row.realtime, RealtimeUsecSet: true, MonotonicUsec: row.realtime, MonotonicUsecSet: true, BootID: row.boot})
		if err != nil {
			t.Fatalf("Append(%s) error = %v", row.message, err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}

	if got := collectBootFirstRealtime([]string{path}, DefaultReaderOptions(), nil); len(got) != 0 {
		t.Fatalf("empty needed boot IDs returned %v, want empty", got)
	}
	got := collectBootFirstRealtime([]string{path, filepath.Join(filepath.Dir(path), "missing.journal")}, DefaultReaderOptions(), map[string]struct{}{
		bootA.String(): {},
	})
	value, ok := got[bootA.String()]
	if !ok || value != 0 {
		t.Fatalf("boot A first realtime = %d, present=%v, want 0/present", value, ok)
	}
	if _, ok := got[bootB.String()]; ok {
		t.Fatalf("unneeded boot B was collected: %v", got)
	}
}

func TestNetdataRequestLimitClampsOversizedValues(t *testing.T) {
	maxInt := maxNativeIntValue()
	beyondInt32 := int64(3_000_000_000)
	wantBeyondInt32 := maxInt
	if int64(maxInt) >= beyondInt32 {
		wantBeyondInt32 = int(beyondInt32)
	}
	cases := []struct {
		name  string
		value any
		want  int
	}{
		{name: "json-number-native-overflow", value: json.Number("9223372036854775808"), want: maxInt},
		{name: "json-number-32-bit-boundary", value: json.Number("3000000000"), want: wantBeyondInt32},
		{name: "uint64-overflow", value: ^uint64(0), want: maxInt},
		{name: "uint-overflow-or-max", value: ^uint(0), want: maxInt},
		{name: "float-overflow", value: float64(maxInt) * 2, want: maxInt},
		{name: "float-native-boundary", value: float64(maxInt), want: maxInt},
		{name: "int64-overflow-on-32-bit", value: int64(3_000_000_000), want: wantBeyondInt32},
		{name: "int-normal", value: 15, want: 15},
		{name: "normal-json-number", value: json.Number("15"), want: 15},
		{name: "zero-uses-default", value: json.Number("0"), want: defaultNetdataItemsToReturn},
		{name: "negative-uses-default", value: json.Number("-1"), want: defaultNetdataItemsToReturn},
		{name: "negative-float-uses-default", value: float64(-1), want: defaultNetdataItemsToReturn},
		{name: "nan-uses-default", value: math.NaN(), want: defaultNetdataItemsToReturn},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := requestLimit(map[string]any{"last": tc.value})
			if got != tc.want {
				t.Fatalf("requestLimit(last=%v) = %d, want %d", tc.value, got, tc.want)
			}
		})
	}
}

func TestNetdataPageWindowRetainsBackwardPage(t *testing.T) {
	backward := &netdataPageWindow{Direction: DirectionBackward, Limit: 2}
	backward.observe(100)
	backward.observe(200)
	backward.observe(300)
	backward.observe(150)
	if backward.Matched != 4 || backward.SkipsAfter != 1 || backward.Shifts != 1 {
		t.Fatalf("backward counters = matched=%d skips_after=%d shifts=%d, want 4/1/1", backward.Matched, backward.SkipsAfter, backward.Shifts)
	}
	if backward.OldestRetainedUsec == nil || backward.NewestRetainedUsec == nil ||
		*backward.OldestRetainedUsec != 200 || *backward.NewestRetainedUsec != 300 {
		t.Fatalf("backward retained bounds = %v/%v, want 200/300", backward.OldestRetainedUsec, backward.NewestRetainedUsec)
	}
	if !backward.candidateToKeep(250) || backward.candidateToKeep(150) {
		t.Fatal("backward candidateToKeep bounds are wrong")
	}
}

func TestNetdataPageWindowRetainsForwardPage(t *testing.T) {
	forward := &netdataPageWindow{Direction: DirectionForward, Limit: 2, Retained: netdataPageHeap{Max: true}}
	forward.observe(300)
	forward.observe(200)
	forward.observe(100)
	forward.observe(250)
	if forward.Matched != 4 || forward.SkipsBefore != 1 || forward.Shifts != 1 {
		t.Fatalf("forward counters = matched=%d skips_before=%d shifts=%d, want 4/1/1", forward.Matched, forward.SkipsBefore, forward.Shifts)
	}
	if forward.OldestRetainedUsec == nil || forward.NewestRetainedUsec == nil ||
		*forward.OldestRetainedUsec != 100 || *forward.NewestRetainedUsec != 200 {
		t.Fatalf("forward retained bounds = %v/%v, want 100/200", forward.OldestRetainedUsec, forward.NewestRetainedUsec)
	}
	if !forward.candidateToKeep(150) || forward.candidateToKeep(250) {
		t.Fatal("forward candidateToKeep bounds are wrong")
	}
}

func TestNetdataPageWindowTailAnchorUsesStopSide(t *testing.T) {
	anchor := uint64(1_700_000_008_000_000)
	request := netdataRequest{
		Anchor:              RealtimeExplorerAnchor(anchor),
		Direction:           DirectionBackward,
		IfModifiedSinceUsec: anchor,
		DataOnly:            true,
		Tail:                true,
		Limit:               2,
	}
	window := newNetdataPageWindow(request)

	for _, value := range []uint64{
		1_700_000_009_000_000,
		1_700_000_008_000_000,
		1_700_000_007_000_000,
	} {
		window.observe(value)
	}

	counters := window.counters()
	if counters.Matched != 1 || counters.Before != 0 || counters.After != 2 {
		t.Fatalf("tail counters = %+v, want matched=1 before=0 after=2", counters)
	}
}

func parseNetdataRequestFixture(t *testing.T) netdataRequest {
	t.Helper()
	request := map[string]any{
		"after":     float64(200_000_000),
		"before":    float64(200_000_100),
		"direction": "forward",
		"last":      float64(1),
		"facets":    []any{"PRIORITY"},
		"histogram": "PRIORITY",
		"query":     `alpha|!debug|needle\|pipe`,
		"selections": map[string]any{
			"PRIORITY":       []any{"warning", "error"},
			"_HOSTNAME":      []any{"node-a"},
			"query":          []any{"ignored"},
			"__logs_sources": []any{"all-local-system-logs", "namespace-blue"},
		},
	}
	parsed, err := parseNetdataRequest(request, SystemdJournalNetdataFunctionConfig())
	if err != nil {
		t.Fatalf("parseNetdataRequest() error = %v", err)
	}
	return parsed
}

func TestNetdataRequestParsingTimeDirectionLimitAndQuery(t *testing.T) {
	parsed := parseNetdataRequestFixture(t)
	if parsed.AfterRealtimeUsec == nil || *parsed.AfterRealtimeUsec != 200_000_000_000_000 {
		t.Fatalf("after = %v, want 200000000000000", parsed.AfterRealtimeUsec)
	}
	if parsed.BeforeRealtimeUsec == nil || *parsed.BeforeRealtimeUsec != 200_000_100_999_999 {
		t.Fatalf("before = %v, want 200000100999999", parsed.BeforeRealtimeUsec)
	}
	if parsed.Direction != DirectionForward {
		t.Fatalf("direction = %v, want forward", parsed.Direction)
	}
	if parsed.Limit != 2 || numericUint64(parsed.Echo["last"]) != 1 {
		t.Fatalf("limit/echo = %d/%v, want effective 2 and echoed 1", parsed.Limit, parsed.Echo["last"])
	}
	if len(parsed.FTSTerms) != 3 || len(parsed.FTSPatterns) != 2 || len(parsed.FTSNegativePatterns) != 1 {
		t.Fatalf("fts terms/patterns/negative = %d/%d/%d, want 3/2/1", len(parsed.FTSTerms), len(parsed.FTSPatterns), len(parsed.FTSNegativePatterns))
	}
}

func TestNetdataRequestParsingFiltersAndSources(t *testing.T) {
	parsed := parseNetdataRequestFixture(t)
	if len(parsed.Filters) != 2 {
		t.Fatalf("filters = %d, want 2", len(parsed.Filters))
	}
	if string(parsed.Filters[0].Field) != "PRIORITY" || string(parsed.Filters[0].Values[0]) != "4" || string(parsed.Filters[0].Values[1]) != "3" {
		t.Fatalf("priority filter = %#v, want warning/error normalized to 4/3", parsed.Filters[0])
	}
	if string(parsed.Filters[1].Field) != "_HOSTNAME" || string(parsed.Filters[1].Values[0]) != "node-a" {
		t.Fatalf("hostname filter = %#v, want _HOSTNAME=node-a", parsed.Filters[1])
	}
	if parsed.SourceType != netdataSourceTypeLocalSystem {
		t.Fatalf("source type = %#x, want local system", parsed.SourceType)
	}
	if len(parsed.ExactSources) != 1 || parsed.ExactSources[0] != "namespace-blue" {
		t.Fatalf("exact sources = %#v, want namespace-blue", parsed.ExactSources)
	}
}

func TestNetdataRequestParsingExplorerQuery(t *testing.T) {
	parsed := parseNetdataRequestFixture(t)
	query := parsed.toExplorerQuery(1, nil, netdataJournalRealtimeDeltaDefault)
	if query.Limit != 2 || query.DebugCollectColumnFieldsByRowTraversal {
		t.Fatalf("explorer query limit/debug = %d/%v, want 2/false", query.Limit, query.DebugCollectColumnFieldsByRowTraversal)
	}
	if !query.ExcludeFacetFieldFilters {
		t.Fatal("multi-filter facet query did not exclude same-field filter")
	}
}

func TestNetdataExplorerSamplingUsesEffectiveAnchorBounds(t *testing.T) {
	before := uint64(1_700_000_010_999_999)
	request := netdataRequest{
		BeforeRealtimeUsec:  &before,
		Anchor:              RealtimeExplorerAnchor(1_700_000_008_000_000),
		Direction:           DirectionBackward,
		Limit:               5,
		DataOnly:            true,
		Delta:               true,
		Tail:                true,
		Sampling:            20,
		IfModifiedSinceUsec: 1_700_000_008_000_000,
	}

	query := request.toExplorerQuery(1, nil, netdataJournalRealtimeDeltaDefault)
	if query.AfterRealtimeUsec == nil || *query.AfterRealtimeUsec != 1_700_000_008_000_001 {
		t.Fatalf("effective after = %v, want anchor+1", query.AfterRealtimeUsec)
	}
	if query.Sampling == nil {
		t.Fatal("sampling disabled, want enabled from effective tail anchor bounds")
	}
}

func TestNormalizeNetdataTimeWindowMatchesPluginEdges(t *testing.T) {
	cases := []struct {
		name       string
		now        int64
		after      *int64
		before     *int64
		wantAfter  uint64
		wantBefore uint64
	}{
		{name: "missing", now: 1_000_000_000, wantAfter: 999_996_400_000_000, wantBefore: 1_000_000_000_999_999},
		{name: "inverted", now: 1_000_000_000, after: i64p(200_000_100), before: i64p(200_000_000), wantAfter: 200_000_000_000_000, wantBefore: 200_000_100_999_999},
		{name: "equal", now: 1_000_000_000, after: i64p(200_000_000), before: i64p(200_000_000), wantAfter: 199_996_400_000_000, wantBefore: 200_000_000_999_999},
		{name: "relative", now: 1_000_000_000, after: i64p(100), before: i64p(200), wantAfter: 999_999_701_000_000, wantBefore: 999_999_800_999_999},
		{name: "missing after", now: 1_000_000_000, before: i64p(200_000_000), wantAfter: 199_999_401_000_000, wantBefore: 200_000_000_999_999},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			after, before := normalizeNetdataTimeWindow(tc.now, tc.after, tc.before)
			if after == nil || before == nil || *after != tc.wantAfter || *before != tc.wantBefore {
				t.Fatalf("normalizeNetdataTimeWindow() = %v/%v, want %d/%d", after, before, tc.wantAfter, tc.wantBefore)
			}
		})
	}
}

func TestNetdataProfileDisplay(t *testing.T) {
	context := newDisplayContext()
	profile := SystemdJournalProfile{}
	if got := profile.FieldDisplayValue(context, DisplayScopeData, "PRIORITY", []byte("7")); got != "debug" {
		t.Fatalf("PRIORITY display = %v, want debug", got)
	}
	if got := profile.FieldDisplayValue(context, DisplayScopeData, "SYSLOG_FACILITY", []byte("3")); got != "daemon" {
		t.Fatalf("SYSLOG_FACILITY display = %v, want daemon", got)
	}
	if got := profile.FieldDisplayValue(context, DisplayScopeFacet, "_UID", []byte("0")); got != "0" {
		t.Fatalf("default _UID display = %v, want raw 0", got)
	}
	fields := map[string][][]byte{"PRIORITY": [][]byte{[]byte("3")}}
	options := anyMap(t, profile.RowOptions(fields))
	if options["severity"] != "critical" {
		t.Fatalf("row severity = %v, want critical", options["severity"])
	}

	plugin := SystemdJournalPluginProfile{}
	missingUID := []byte("999999999")
	if got := plugin.FieldDisplayValue(context, DisplayScopeFacet, "_UID", missingUID); got != string(missingUID) {
		t.Fatalf("plugin missing _UID display = %v, want raw fallback", got)
	}
	if got := plugin.FieldDisplayValue(context, DisplayScopeData, "_UID", missingUID); got != string(missingUID) {
		t.Fatalf("plugin cached missing _UID display = %v, want raw fallback", got)
	}
	if len(context.uidDisplayCache) != 1 {
		t.Fatalf("uid display cache size = %d, want 1", len(context.uidDisplayCache))
	}
}

func TestNetdataDynamicProcessNameMatchesPluginFallbackOrder(t *testing.T) {
	fields := map[string][][]byte{
		"SYSLOG_IDENTIFIER": [][]byte{[]byte("syslog")},
		"_COMM":             [][]byte{[]byte("comm")},
		"_PID":              [][]byte{[]byte("42")},
		"SYSLOG_PID":        [][]byte{[]byte("99")},
	}
	if got := dynamicProcessName(fields); got != "syslog[42]" {
		t.Fatalf("dynamicProcessName(syslog pid) = %q, want syslog[42]", got)
	}

	fields["CONTAINER_NAME"] = [][]byte{[]byte("container")}
	if got := dynamicProcessName(fields); got != "container[42]" {
		t.Fatalf("dynamicProcessName(container pid) = %q, want container[42]", got)
	}

	delete(fields, "CONTAINER_NAME")
	delete(fields, "SYSLOG_IDENTIFIER")
	delete(fields, "_PID")
	if got := dynamicProcessName(fields); got != "comm[-]" {
		t.Fatalf("dynamicProcessName(missing pid) = %q, want comm[-]", got)
	}

	fields["_PID"] = [][]byte{[]byte("")}
	if got := dynamicProcessName(fields); got != "comm" {
		t.Fatalf("dynamicProcessName(empty pid) = %q, want comm", got)
	}

	delete(fields, "_COMM")
	delete(fields, "_PID")
	fields["_EXE"] = [][]byte{[]byte("/usr/bin/app")}
	if got := dynamicProcessName(fields); got != "-" {
		t.Fatalf("dynamicProcessName(no identifier) = %q, want -", got)
	}
}

func TestNetdataRealtimeAdjustment(t *testing.T) {
	forward := newNetdataRealtimeAdjuster(DirectionForward)
	if got := []uint64{forward.adjust(10), forward.adjust(10), forward.adjust(10)}; got[0] != 10 || got[1] != 11 || got[2] != 12 {
		t.Fatalf("forward realtime adjustment = %v, want 10/11/12", got)
	}
	backward := newNetdataRealtimeAdjuster(DirectionBackward)
	if got := []uint64{backward.adjust(10), backward.adjust(10), backward.adjust(10)}; got[0] != 10 || got[1] != 9 || got[2] != 8 {
		t.Fatalf("backward realtime adjustment = %v, want 10/9/8", got)
	}
}

func TestNetdataDataOnlyDeltaTailSamplingAndNoChangeModes(t *testing.T) {
	path := createExplorerManyJournal(t, 500)
	dir := filepath.Dir(path)
	function := SystemdJournalPluginCompatibleNetdataFunction()

	dataOnly, err := function.RunDirectoryRequestJSONWithOptions(dir, map[string]any{
		"after":     float64(1_700_000_000),
		"before":    float64(1_800_000_000),
		"data_only": true,
		"last":      float64(5),
		"sampling":  float64(20),
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(data_only) error = %v", err)
	}
	if _, ok := dataOnly["_sampling"]; ok {
		t.Fatal("data_only response has _sampling, want disabled")
	}
	if _, ok := dataOnly["facets"]; ok {
		t.Fatal("data_only response has full facets, want data-only response")
	}

	delta, err := function.RunDirectoryRequestJSONWithOptions(dir, map[string]any{
		"after":     float64(1_700_000_000),
		"before":    float64(1_800_000_000),
		"data_only": true,
		"delta":     true,
		"facets":    []any{"SERVICE"},
		"histogram": "SERVICE",
		"last":      float64(5),
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(delta) error = %v", err)
	}
	if _, ok := delta["facets_delta"]; !ok {
		t.Fatal("delta response missing facets_delta")
	}
	if _, ok := delta["items_delta"]; !ok {
		t.Fatal("delta response missing items_delta")
	}

	noChange, err := function.RunDirectoryRequestJSONWithOptions(dir, map[string]any{
		"after":             float64(1_700_000_000),
		"before":            float64(1_800_000_000),
		"if_modified_since": float64(9_999_999_999_999_999),
		"data_only":         true,
		"last":              float64(5),
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(no-change) error = %v", err)
	}
	if got := numericUint64(noChange["status"]); got != 304 {
		t.Fatalf("no-change status = %d, want 304", got)
	}

	tail, err := function.RunDirectoryRequestJSONWithOptions(dir, map[string]any{
		"after":             float64(1_700_000_000),
		"before":            float64(1_800_000_000),
		"if_modified_since": float64(1),
		"anchor":            float64(1_700_000_000_000_250),
		"data_only":         true,
		"tail":              true,
		"last":              float64(5),
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(tail) error = %v", err)
	}
	echo := anyMap(t, tail["_request"])
	if echo["tail"] != true || echo["direction"] != "backward" {
		t.Fatalf("tail echo = %#v, want tail=true direction=backward", echo)
	}
}

func TestNetdataTailAnchorWithNewerFilteredOutRowsReturnsEmpty200(t *testing.T) {
	startRealtime := uint64(1_700_000_000_000_000)
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{
			realtime: startRealtime,
			payloads: [][]byte{
				[]byte("MESSAGE=old-visible"),
				[]byte("SERVICE=keep"),
			},
		},
		{
			realtime: startRealtime + 1_000_000,
			payloads: [][]byte{
				[]byte("MESSAGE=new-filtered-out"),
				[]byte("SERVICE=other"),
			},
		},
	})
	function := SystemdJournalPluginCompatibleNetdataFunction()

	response, err := function.RunDirectoryRequestJSONWithOptions(filepath.Dir(path), map[string]any{
		"after":             float64(1_700_000_000),
		"before":            float64(1_700_000_002),
		"anchor":            float64(startRealtime),
		"if_modified_since": float64(startRealtime),
		"data_only":         true,
		"tail":              true,
		"last":              float64(5),
		"selections": map[string]any{
			"SERVICE": []any{"keep"},
		},
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(tail no new filtered rows) error = %v", err)
	}
	if got := numericUint64(response["status"]); got != 200 {
		t.Fatalf("filtered tail status = %d, want 200 (response=%#v)", got, response)
	}
	if got := responseColumnStrings(t, response, "MESSAGE"); len(got) != 0 {
		t.Fatalf("filtered tail rows = %v, want empty", got)
	}
}

func TestNetdataFunctionPagesWithAnchorWithoutDuplicateOrMissingRows(t *testing.T) {
	const startRealtime = uint64(1_700_000_000_000_000)
	path := createExplorerManyJournal(t, 7)
	dir := filepath.Dir(path)

	backward := collectNetdataPages(t, dir, "backward", 2)
	assertStringSlice(t, backward.messages, []string{"row-6", "row-5", "row-4", "row-3", "row-2", "row-1", "row-0"})
	assertUniqueMessages(t, backward.messages)
	assertUint64Slice(t, backward.timestamps, []uint64{
		startRealtime + 6,
		startRealtime + 5,
		startRealtime + 4,
		startRealtime + 3,
		startRealtime + 2,
		startRealtime + 1,
		startRealtime,
	})

	forward := collectNetdataPages(t, dir, "forward", 2)
	assertStringSlice(t, forward.messages, []string{"row-1", "row-0", "row-3", "row-2", "row-5", "row-4", "row-6"})
	assertUniqueMessages(t, forward.messages)
	assertUint64Slice(t, forward.timestamps, []uint64{
		startRealtime + 1,
		startRealtime,
		startRealtime + 3,
		startRealtime + 2,
		startRealtime + 5,
		startRealtime + 4,
		startRealtime + 6,
	})
}

func TestNetdataFunctionTailPollsReturnOnlyRowsAfterAnchorThen304(t *testing.T) {
	const startRealtime = uint64(1_700_000_000_000_000)
	path := createExplorerManyJournal(t, 5)
	dir := filepath.Dir(path)
	anchor := startRealtime + 2

	for _, requestedDirection := range []string{"backward", "forward"} {
		response := runNetdataContractRequest(t, dir, map[string]any{
			"after":             float64(1_700_000_000),
			"before":            float64(1_700_000_010),
			"last":              float64(5),
			"direction":         requestedDirection,
			"data_only":         true,
			"tail":              true,
			"if_modified_since": float64(anchor),
			"anchor":            float64(anchor),
		})
		if got := numericUint64(response["status"]); got != 200 {
			t.Fatalf("tail status = %d, want 200 (response=%#v)", got, response)
		}
		echo := anyMap(t, response["_request"])
		if echo["direction"] != "backward" {
			t.Fatalf("tail direction echo = %v, want backward", echo["direction"])
		}
		assertStringSlice(t, responseColumnStrings(t, response, "MESSAGE"), []string{"row-4", "row-3"})
		assertUint64Slice(t, responseColumnUint64s(t, response, "timestamp"), []uint64{startRealtime + 4, startRealtime + 3})
	}

	noChange := runNetdataContractRequest(t, dir, map[string]any{
		"after":             float64(1_700_000_000),
		"before":            float64(1_700_000_010),
		"last":              float64(5),
		"direction":         "backward",
		"data_only":         true,
		"tail":              true,
		"if_modified_since": float64(startRealtime + 4),
		"anchor":            float64(startRealtime + 4),
	})
	if got := numericUint64(noChange["status"]); got != 304 {
		t.Fatalf("tail no-change status = %d, want 304 (response=%#v)", got, noChange)
	}
	if noChange["errorMessage"] != "No new data since the previous call." {
		t.Fatalf("tail no-change error = %v", noChange["errorMessage"])
	}
}

func TestNetdataFunctionTailDeltaReportsExactIncrementalFacetsAndHistogram(t *testing.T) {
	const startRealtime = uint64(1_700_000_000_000_000)
	path := createExplorerManyJournal(t, 5)
	anchor := startRealtime + 1

	response := runNetdataContractRequest(t, filepath.Dir(path), map[string]any{
		"after":             float64(1_700_000_000),
		"before":            float64(1_700_000_010),
		"last":              float64(2),
		"direction":         "backward",
		"data_only":         true,
		"delta":             true,
		"tail":              true,
		"if_modified_since": float64(anchor),
		"anchor":            float64(anchor),
		"facets":            []any{"SERVICE"},
		"histogram":         "SERVICE",
	})

	if got := numericUint64(response["status"]); got != 200 {
		t.Fatalf("tail delta status = %d, want 200 (response=%#v)", got, response)
	}
	assertStringSlice(t, responseColumnStrings(t, response, "MESSAGE"), []string{"row-4", "row-3"})
	if got := responseFacetCount(t, response, "facets_delta", "SERVICE", "even"); got != 2 {
		t.Fatalf("facets_delta SERVICE=even = %d, want 2", got)
	}
	if got := responseFacetCount(t, response, "facets_delta", "SERVICE", "odd"); got != 1 {
		t.Fatalf("facets_delta SERVICE=odd = %d, want 1", got)
	}
	if got := responseHistogramTotal(t, response, "histogram_delta", "even"); got != 2 {
		t.Fatalf("histogram_delta even = %d, want 2", got)
	}
	if got := responseHistogramTotal(t, response, "histogram_delta", "odd"); got != 1 {
		t.Fatalf("histogram_delta odd = %d, want 1", got)
	}
	items := anyMap(t, response["items_delta"])
	if got := numericUint64(items["matched"]); got != 3 {
		t.Fatalf("items_delta.matched = %d, want 3", got)
	}
	if got := numericUint64(items["returned"]); got != 2 {
		t.Fatalf("items_delta.returned = %d, want 2", got)
	}
	if got := numericUint64(items["after"]); got != 2 {
		t.Fatalf("items_delta.after = %d, want 2", got)
	}
}

func TestNetdataFunctionReportsProgressTimeoutAndSamplingCounters(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)
	dir := filepath.Dir(path)
	function := SystemdJournalPluginCompatibleNetdataFunction()
	request := map[string]any{
		"after":     float64(1_700_000_000),
		"before":    float64(1_800_000_000),
		"facets":    []any{"SERVICE"},
		"histogram": "SERVICE",
		"last":      float64(0),
	}

	var progressReports int
	options := DefaultNetdataFunctionRunOptions()
	options.ProgressInterval = 0
	options.ProgressCallback = func(progress NetdataFunctionProgress) {
		progressReports++
		if progress.CurrentFile != 1 || progress.TotalFiles != 1 {
			t.Fatalf("progress file counters = %d/%d, want 1/1", progress.CurrentFile, progress.TotalFiles)
		}
	}
	response, err := function.RunDirectoryRequestJSONWithOptions(dir, request, options)
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(progress) error = %v", err)
	}
	if got := numericUint64(response["status"]); got != 200 {
		t.Fatalf("progress response status = %d, want 200", got)
	}
	if progressReports == 0 {
		t.Fatal("progress callback was not called")
	}

	timeout := time.Duration(0)
	timeoutOptions := DefaultNetdataFunctionRunOptions()
	timeoutOptions.Timeout = &timeout
	timeoutResponse, err := function.RunDirectoryRequestJSONWithOptions(dir, request, timeoutOptions)
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(timeout) error = %v", err)
	}
	if got := numericUint64(timeoutResponse["status"]); got != 200 {
		t.Fatalf("timeout response status = %d, want partial table status 200", got)
	}
	if timeoutResponse["partial"] != true {
		t.Fatalf("timeout partial = %v, want true", timeoutResponse["partial"])
	}
	message := anyMap(t, timeoutResponse["message"])
	if message["status"] != "warning" {
		t.Fatalf("timeout message status = %v, want warning", message["status"])
	}
}

func TestNetdataFunctionReportsSamplingCounters(t *testing.T) {
	base := uint64(1_700_000_000_000_000)
	entries := make([]explorerTestEntry, 0, 5_000)
	for i := 0; i < 5_000; i++ {
		service := "odd"
		if i%2 == 0 {
			service = "even"
		}
		entries = append(entries, explorerTestEntry{
			realtime: base + uint64(i)*1_000,
			payloads: [][]byte{
				[]byte("MESSAGE=sampled"),
				[]byte("SERVICE=" + service),
			},
		})
	}
	path := createExplorerRawJournal(t, entries)
	response, err := SystemdJournalPluginCompatibleNetdataFunction().RunDirectoryRequestJSONWithOptions(filepath.Dir(path), map[string]any{
		"after":     float64(1_700_000_000),
		"before":    float64(1_700_000_005),
		"facets":    []any{"SERVICE"},
		"histogram": "SERVICE",
		"last":      float64(5),
		"sampling":  float64(20),
	}, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(sampling) error = %v", err)
	}
	sampling := anyMap(t, response["_sampling"])
	if sampling["enabled"] != true {
		t.Fatalf("sampling enabled = %v, want true", sampling["enabled"])
	}
	if numericUint64(sampling["sampled"]) == 0 || numericUint64(sampling["unsampled"]) == 0 || numericUint64(sampling["estimated"]) == 0 {
		t.Fatalf("sampling counters = %#v, want sampled/unsampled/estimated all positive", sampling)
	}
	items := anyMap(t, response["items"])
	if numericUint64(items["estimated"]) != numericUint64(sampling["estimated"]) {
		t.Fatalf("items estimated = %v, sampling estimated = %v", items["estimated"], sampling["estimated"])
	}
}

func TestNetdataCollectJournalFilesRecursesAndClassifiesSources(t *testing.T) {
	root := t.TempDir()
	systemPath := filepath.Join(root, "system.journal")
	userDir := filepath.Join(root, "user")
	if err := os.MkdirAll(userDir, 0o755); err != nil {
		t.Fatalf("MkdirAll(user) error = %v", err)
	}
	userPath := filepath.Join(userDir, "user-1000.journal")
	writeNetdataTestJournalAt(t, systemPath, "system")
	writeNetdataTestJournalAt(t, userPath, "user")
	if err := os.WriteFile(filepath.Join(root, "ignored.txt"), []byte("not a journal"), 0o644); err != nil {
		t.Fatalf("WriteFile(ignored) error = %v", err)
	}

	collection, err := collectNetdataJournalFiles(root)
	if err != nil {
		t.Fatalf("collectNetdataJournalFiles() error = %v", err)
	}
	if len(collection.Files) != 2 {
		t.Fatalf("collected files = %#v, want 2 journal files", collection.Files)
	}
	request, err := parseNetdataRequest(map[string]any{
		"selections": map[string]any{"__logs_sources": []any{"all-local-system-logs"}},
	}, SystemdJournalNetdataFunctionConfig())
	if err != nil {
		t.Fatalf("parseNetdataRequest(source) error = %v", err)
	}
	if !request.matchesSource(systemPath, nil) {
		t.Fatal("system.journal did not match all-local-system-logs")
	}
	if request.matchesSource(userPath, nil) {
		t.Fatal("user journal matched all-local-system-logs")
	}

	namespacePath := filepath.Join(root, "ns.blue", "system.journal")
	if got := localNamespaceSourceName(namespacePath); got != "namespace-blue" {
		t.Fatalf("namespace source = %q, want namespace-blue", got)
	}
	remotePath := filepath.Join(root, "remote", "remote-node@abc.journal")
	if got := journalFileExactSourceName(remotePath); got != "remote-node" {
		t.Fatalf("remote exact source = %q, want remote-node", got)
	}
}

func TestNetdataFunctionQueryFiltersFacetsHistogramAndRows(t *testing.T) {
	base := uint64(1_700_000_000_000_000)
	path := createExplorerRawJournal(t, []explorerTestEntry{
		{realtime: base + 1_000_000, payloads: [][]byte{
			[]byte("MESSAGE=alpha startup"),
			[]byte("PRIORITY=3"),
			[]byte("_SYSTEMD_UNIT=alpha.service"),
			[]byte("SYSLOG_IDENTIFIER=alpha"),
			[]byte("_TRANSPORT=stdout"),
		}},
		{realtime: base + 2_000_000, payloads: [][]byte{
			[]byte("MESSAGE=beta ignored"),
			[]byte("PRIORITY=4"),
			[]byte("_SYSTEMD_UNIT=beta.service"),
			[]byte("SYSLOG_IDENTIFIER=beta"),
			[]byte("_TRANSPORT=journal"),
		}},
		{realtime: base + 3_000_000, payloads: [][]byte{
			[]byte("MESSAGE=alpha failed"),
			[]byte("PRIORITY=3"),
			[]byte("_SYSTEMD_UNIT=alpha.service"),
			[]byte("SYSLOG_IDENTIFIER=alpha"),
			[]byte("_TRANSPORT=stdout"),
		}},
	})

	request := map[string]any{
		"after":     float64(1_700_000_000),
		"before":    float64(1_700_000_010),
		"last":      float64(5),
		"facets":    []any{"PRIORITY", "_SYSTEMD_UNIT"},
		"histogram": "PRIORITY",
		"query":     "alpha",
		"selections": map[string]any{
			"PRIORITY": []any{"err"},
		},
	}

	response, err := SystemdJournalPluginCompatibleNetdataFunction().
		RunDirectoryRequestJSONWithOptions(filepath.Dir(path), request, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(query) error = %v", err)
	}
	if got := response["status"]; got != 200 {
		t.Fatalf("status = %v, want 200", got)
	}
	if got := len(anySlice(t, response["data"])); got != 2 {
		t.Fatalf("data rows = %d, want 2", got)
	}
	assertNetdataFacetCount(t, response, "PRIORITY", "3", 2)
	assertNetdataFacetCount(t, response, "_SYSTEMD_UNIT", "alpha.service", 2)
	histogram, ok := response["histogram"].(map[string]any)
	if !ok {
		t.Fatalf("histogram = %T, want object", response["histogram"])
	}
	if got := histogram["id"]; got != "PRIORITY" {
		t.Fatalf("histogram.id = %v, want PRIORITY", got)
	}
}

func TestNetdataHistogramChartMetadataIncludesDimensionArrays(t *testing.T) {
	function := SystemdJournalPluginCompatibleNetdataFunction()
	empty := function.buildHistogram(newDisplayContext(), &ExplorerHistogram{
		Field: []byte("TRAP_SEVERITY"),
		Buckets: []ExplorerHistogramBucket{{
			StartRealtimeUsec: 1_700_000_000_000_000,
			EndRealtimeUsec:   1_700_000_005_000_000,
			Values:            map[string]uint64{},
		}},
	}, nil)
	emptyChart := anyMap(t, anyMap(t, empty)["chart"])
	emptyViewDimensions := anyMap(t, anyMap(t, emptyChart["view"])["dimensions"])
	if got := anySlice(t, emptyViewDimensions["names"]); len(got) != 0 {
		t.Fatalf("empty histogram view dimensions names = %v, want empty array", got)
	}
	if got := anySlice(t, emptyViewDimensions["ids"]); len(got) != 0 {
		t.Fatalf("empty histogram view dimensions ids = %v, want empty array", got)
	}
	emptyDBDimensions := anyMap(t, anyMap(t, emptyChart["db"])["dimensions"])
	if got := anySlice(t, emptyDBDimensions["names"]); len(got) != 0 {
		t.Fatalf("empty histogram db dimensions names = %v, want empty array", got)
	}

	withValue := function.buildHistogram(newDisplayContext(), &ExplorerHistogram{
		Field: []byte("TRAP_SEVERITY"),
		Buckets: []ExplorerHistogramBucket{{
			StartRealtimeUsec: 1_700_000_000_000_000,
			EndRealtimeUsec:   1_700_000_005_000_000,
			Values:            map[string]uint64{"warning": 7},
		}},
	}, nil)
	valueChart := anyMap(t, anyMap(t, withValue)["chart"])
	valueViewDimensions := anyMap(t, anyMap(t, valueChart["view"])["dimensions"])
	if got := anySlice(t, valueViewDimensions["names"]); len(got) != 1 || got[0] != "warning" {
		t.Fatalf("histogram view dimension names = %v, want [warning]", got)
	}
	if got := anySlice(t, anyMap(t, valueViewDimensions["sts"])["min"]); len(got) != 1 || numericUint64(got[0]) != 7 {
		t.Fatalf("histogram view dimension min = %v, want [7]", got)
	}
}

func TestNetdataSourceSummaryCoverageUsesOffBelowOneSecond(t *testing.T) {
	first := uint64(1_700_000_000_000_000)
	subSecond := first + 999_999
	oneSecond := first + 1_000_000
	summary := netdataJournalSourceSummary{
		Files:             1,
		FirstRealtimeUsec: &first,
		LastRealtimeUsec:  &subSecond,
	}

	if got := summary.info(); !strings.Contains(got, "covering off") {
		t.Fatalf("summary.info() = %q, want coverage off", got)
	}

	summary.LastRealtimeUsec = &oneSecond
	if got := summary.info(); !strings.Contains(got, "covering 1s") {
		t.Fatalf("summary.info() = %q, want coverage 1s", got)
	}
}

func TestNetdataFunctionRequestBytesAndCancellation(t *testing.T) {
	path := createExplorerManyJournal(t, 9_000)
	request := []byte(`{"after":1700000000,"before":1800000000,"last":5,"facets":["SERVICE"],"histogram":"SERVICE"}`)
	var progressReports int
	options := DefaultNetdataFunctionRunOptions()
	options.ProgressInterval = 0
	options.ProgressCallback = func(NetdataFunctionProgress) {
		progressReports++
	}
	options.CancellationCallback = func() bool {
		return progressReports > 0
	}
	response, err := SystemdJournalPluginCompatibleNetdataFunction().
		RunDirectoryRequestBytesWithOptions(filepath.Dir(path), request, options)
	if err != nil {
		t.Fatalf("RunDirectoryRequestBytesWithOptions(cancel) error = %v", err)
	}
	if progressReports == 0 {
		t.Fatal("progress callback was not called")
	}
	if got := numericUint64(response["status"]); got != 499 {
		t.Fatalf("status = %v, want 499", got)
	}
}

func TestNetdataFunctionRejectsInvalidRequestJSON(t *testing.T) {
	_, err := SystemdJournalPluginCompatibleNetdataFunction().
		RunDirectoryRequestBytesWithOptions(t.TempDir(), []byte(`{"info":`), DefaultNetdataFunctionRunOptions())
	if err == nil {
		t.Fatal("RunDirectoryRequestBytesWithOptions(invalid JSON) error = nil, want error")
	}
	if !errors.Is(err, ErrInvalidJournal) {
		t.Fatalf("error = %T %[1]v, want ErrInvalidJournal", err)
	}
	if !strings.Contains(err.Error(), "invalid Netdata function JSON") {
		t.Fatalf("error = %v, want invalid JSON context", err)
	}
}

func assertNetdataFacetCount(t *testing.T, response map[string]any, field, value string, want uint64) {
	t.Helper()
	for _, facetAny := range anySlice(t, response["facets"]) {
		facet := anyMap(t, facetAny)
		if facet["id"] != field {
			continue
		}
		for _, optionAny := range anySlice(t, facet["options"]) {
			option := anyMap(t, optionAny)
			if option["id"] == value {
				if numericUint64(option["count"]) != want {
					t.Fatalf("facet %s=%s count = %v, want %v", field, value, option["count"], want)
				}
				return
			}
		}
		t.Fatalf("facet %s missing value %s", field, value)
	}
	t.Fatalf("missing facet %s", field)
}

type netdataCollectedPages struct {
	messages   []string
	timestamps []uint64
}

func collectNetdataPages(t *testing.T, dir, direction string, pageSize int) netdataCollectedPages {
	t.Helper()
	var out netdataCollectedPages
	var anchor *uint64
	for page := 0; page < 100; page++ {
		request := map[string]any{
			"after":     float64(1_700_000_000),
			"before":    float64(1_800_000_000),
			"last":      float64(pageSize),
			"direction": direction,
			"data_only": true,
		}
		if anchor != nil {
			request["anchor"] = float64(*anchor)
		}
		response := runNetdataContractRequest(t, dir, request)
		if got := numericUint64(response["status"]); got != 200 {
			t.Fatalf("page %d status = %d, want 200 (response=%#v)", page, got, response)
		}
		messages := responseColumnStrings(t, response, "MESSAGE")
		timestamps := responseColumnUint64s(t, response, "timestamp")
		if len(messages) != len(timestamps) {
			t.Fatalf("page %d messages=%d timestamps=%d", page, len(messages), len(timestamps))
		}
		if len(messages) == 0 {
			break
		}
		out.messages = append(out.messages, messages...)
		out.timestamps = append(out.timestamps, timestamps...)
		nextAnchor := timestamps[len(timestamps)-1]
		if direction == "forward" {
			nextAnchor = timestamps[0]
		}
		anchor = &nextAnchor
		if len(messages) < pageSize {
			break
		}
	}
	if len(out.messages) == 0 {
		t.Fatal("pagination returned no rows")
	}
	return out
}

func runNetdataContractRequest(t *testing.T, dir string, request map[string]any) map[string]any {
	t.Helper()
	response, err := SystemdJournalPluginCompatibleNetdataFunction().
		RunDirectoryRequestJSONWithOptions(dir, request, DefaultNetdataFunctionRunOptions())
	if err != nil {
		t.Fatalf("RunDirectoryRequestJSONWithOptions(%#v) error = %v", request, err)
	}
	return response
}

func responseColumnStrings(t *testing.T, response map[string]any, field string) []string {
	t.Helper()
	index := responseColumnIndex(t, response, field)
	rows := anySlice(t, response["data"])
	out := make([]string, 0, len(rows))
	for _, rowAny := range rows {
		row := anySlice(t, rowAny)
		if index >= len(row) {
			t.Fatalf("field %s index %d outside row with %d columns", field, index, len(row))
		}
		value, ok := row[index].(string)
		if !ok {
			t.Fatalf("field %s value = %T, want string", field, row[index])
		}
		out = append(out, value)
	}
	return out
}

func responseColumnUint64s(t *testing.T, response map[string]any, field string) []uint64 {
	t.Helper()
	index := responseColumnIndex(t, response, field)
	rows := anySlice(t, response["data"])
	out := make([]uint64, 0, len(rows))
	for _, rowAny := range rows {
		row := anySlice(t, rowAny)
		if index >= len(row) {
			t.Fatalf("field %s index %d outside row with %d columns", field, index, len(row))
		}
		out = append(out, numericUint64(row[index]))
	}
	return out
}

func responseColumnIndex(t *testing.T, response map[string]any, field string) int {
	t.Helper()
	columns := anyMap(t, response["columns"])
	column := anyMap(t, columns[field])
	return int(numericUint64(column["index"]))
}

func responseFacetCount(t *testing.T, response map[string]any, key, field, value string) uint64 {
	t.Helper()
	for _, facetAny := range anySlice(t, response[key]) {
		facet := anyMap(t, facetAny)
		if facet["id"] != field {
			continue
		}
		for _, optionAny := range anySlice(t, facet["options"]) {
			option := anyMap(t, optionAny)
			if option["id"] == value {
				return numericUint64(option["count"])
			}
		}
		t.Fatalf("%s facet %s missing value %s", key, field, value)
	}
	t.Fatalf("%s missing facet %s", key, field)
	return 0
}

func responseHistogramTotal(t *testing.T, response map[string]any, key, value string) uint64 {
	t.Helper()
	histogram := anyMap(t, response[key])
	chart := anyMap(t, histogram["chart"])
	view := anyMap(t, chart["view"])
	dimensions := anyMap(t, view["dimensions"])
	names := anySlice(t, dimensions["names"])
	dimensionIndex := -1
	for index, nameAny := range names {
		if nameAny == value {
			dimensionIndex = index
			break
		}
	}
	if dimensionIndex == -1 {
		t.Fatalf("%s histogram missing dimension %s", key, value)
	}
	result := anyMap(t, chart["result"])
	var total uint64
	for _, pointAny := range anySlice(t, result["data"]) {
		point := anySlice(t, pointAny)
		cellIndex := dimensionIndex + 1
		if cellIndex >= len(point) {
			t.Fatalf("%s histogram point has %d cells, need index %d", key, len(point), cellIndex)
		}
		cell := anySlice(t, point[cellIndex])
		if len(cell) == 0 {
			t.Fatalf("%s histogram point cell is empty", key)
		}
		total += numericUint64(cell[0])
	}
	return total
}

func assertStringSlice(t *testing.T, got, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("strings = %v, want %v", got, want)
	}
	for index := range got {
		if got[index] != want[index] {
			t.Fatalf("strings = %v, want %v", got, want)
		}
	}
}

func assertUint64Slice(t *testing.T, got, want []uint64) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("uint64s = %v, want %v", got, want)
	}
	for index := range got {
		if got[index] != want[index] {
			t.Fatalf("uint64s = %v, want %v", got, want)
		}
	}
}

func assertUniqueMessages(t *testing.T, messages []string) {
	t.Helper()
	seen := make(map[string]struct{}, len(messages))
	for _, message := range messages {
		if _, ok := seen[message]; ok {
			t.Fatalf("duplicate message %q in %v", message, messages)
		}
		seen[message] = struct{}{}
	}
}

func anySlice(t *testing.T, value any) []any {
	t.Helper()
	slice, ok := value.([]any)
	if !ok {
		t.Fatalf("value = %T, want []any", value)
	}
	return slice
}

func anyMap(t *testing.T, value any) map[string]any {
	t.Helper()
	object, ok := value.(map[string]any)
	if !ok {
		t.Fatalf("value = %T, want map[string]any", value)
	}
	return object
}

func numericUint64(value any) uint64 {
	switch typed := value.(type) {
	case uint64:
		return typed
	case uint:
		return uint64(typed)
	case int:
		if typed < 0 {
			return 0
		}
		return uint64(typed)
	case int64:
		if typed < 0 {
			return 0
		}
		return uint64(typed)
	case float64:
		if typed < 0 {
			return 0
		}
		return uint64(typed)
	default:
		return 0
	}
}

func i64p(value int64) *int64 {
	return &value
}

func writeNetdataTestJournalAt(t *testing.T, path, message string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%s) error = %v", filepath.Dir(path), err)
	}
	writer, err := Create(path, testOptions())
	if err != nil {
		t.Fatalf("Create(%s) error = %v", path, err)
	}
	if err := writer.Append([]Field{
		StringField("MESSAGE", message),
		StringField("SERVICE", "unit-test"),
	}, EntryOptions{RealtimeUsec: 1_700_000_000_000_000, RealtimeUsecSet: true, MonotonicUsec: 1, MonotonicUsecSet: true}); err != nil {
		t.Fatalf("Append(%s) error = %v", path, err)
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("Close(%s) error = %v", path, err)
	}
}
