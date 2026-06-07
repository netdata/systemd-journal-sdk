package journal

import (
	"errors"
	"path/filepath"
	"strings"
	"testing"
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
