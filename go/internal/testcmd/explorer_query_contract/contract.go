package explorerquerycontract

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"time"
)

const QuerySchema = "systemd-journal-sdk-explorer-query-v1"
const ReportSchema = "systemd-journal-sdk-explorer-report-v1"

type QuerySpec struct {
	Schema              string        `json:"schema"`
	Name                string        `json:"name"`
	Mode                QueryMode     `json:"mode"`
	Filters             []FilterSpec  `json:"filters"`
	Facets              []ValueSpec   `json:"facets"`
	Display             DisplaySpec   `json:"display"`
	DisplayFields       []ValueSpec   `json:"display_fields"`
	FullText            *ValueSpec    `json:"full_text"`
	UniqueField         *ValueSpec    `json:"unique_field"`
	UniqueIncludeCounts bool          `json:"unique_include_counts"`
	UniqueSkip          int           `json:"unique_skip"`
	Limit               *int          `json:"limit"`
	Direction           DirectionSpec `json:"direction"`
	SinceRealtimeUsec   *uint64       `json:"since_realtime_usec"`
	UntilRealtimeUsec   *uint64       `json:"until_realtime_usec"`
}

type QueryMode string

const (
	QueryModeQuery  QueryMode = "query"
	QueryModeUnique QueryMode = "unique"
)

type FilterSpec struct {
	Field  ValueSpec   `json:"field"`
	Op     FilterOp    `json:"op"`
	Values []ValueSpec `json:"values"`
}

type FilterOp string

const (
	FilterOpIn    FilterOp = "in"
	FilterOpNotIn FilterOp = "not-in"
)

type DirectionSpec string

const (
	DirectionForward  DirectionSpec = "forward"
	DirectionBackward DirectionSpec = "backward"
)

type DisplaySpec string

const (
	DisplayNone   DisplaySpec = "none"
	DisplayAll    DisplaySpec = "all"
	DisplayFields DisplaySpec = "fields"
)

type ValueSpec struct {
	Text *string
	Hex  *string
}

func (v *ValueSpec) UnmarshalJSON(data []byte) error {
	var text string
	if err := json.Unmarshal(data, &text); err == nil {
		v.Text = &text
		v.Hex = nil
		return nil
	}
	var object struct {
		Text *string `json:"text"`
		Hex  *string `json:"hex"`
	}
	if err := json.Unmarshal(data, &object); err != nil {
		return err
	}
	if (object.Text == nil && object.Hex == nil) || (object.Text != nil && object.Hex != nil) {
		return fmt.Errorf("value object must set exactly one of text or hex")
	}
	v.Text = object.Text
	v.Hex = object.Hex
	return nil
}

func (v ValueSpec) Bytes() ([]byte, error) {
	if v.Text != nil {
		return []byte(*v.Text), nil
	}
	if v.Hex != nil {
		decoded, err := hex.DecodeString(*v.Hex)
		if err != nil {
			return nil, err
		}
		return decoded, nil
	}
	return nil, fmt.Errorf("value object must set text or hex")
}

type QueryReport struct {
	Schema         string            `json:"schema"`
	Engine         string            `json:"engine"`
	Query          string            `json:"query"`
	Input          string            `json:"input"`
	ElapsedSeconds string            `json:"elapsed_seconds"`
	Rows           []RowReport       `json:"rows"`
	Facets         []FacetReport     `json:"facets"`
	UniqueValues   []UniqueReport    `json:"unique_values"`
	Counters       map[string]uint64 `json:"counters"`
}

type RowReport struct {
	Realtime uint64        `json:"realtime"`
	Seqnum   uint64        `json:"seqnum"`
	Cursor   string        `json:"cursor"`
	Fields   []FieldReport `json:"fields"`
}

type FieldReport struct {
	NameHex  string `json:"name_hex"`
	ValueHex string `json:"value_hex"`
}

type FacetReport struct {
	FieldHex string             `json:"field_hex"`
	Values   []FacetValueReport `json:"values"`
}

type FacetValueReport struct {
	ValueHex string `json:"value_hex"`
	Count    uint64 `json:"count"`
}

type UniqueReport struct {
	ValueHex string  `json:"value_hex"`
	Count    *uint64 `json:"count"`
}

func ReadQuery(path string) (QuerySpec, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return QuerySpec{}, err
	}
	var query QuerySpec
	if err := json.Unmarshal(raw, &query); err != nil {
		return QuerySpec{}, err
	}
	if query.Schema != QuerySchema {
		return QuerySpec{}, fmt.Errorf("unsupported query schema %q", query.Schema)
	}
	if query.Mode == "" {
		query.Mode = QueryModeQuery
	}
	if query.Display == "" {
		query.Display = DisplayAll
	}
	if query.Direction == "" {
		query.Direction = DirectionForward
	}
	return query, nil
}

func WriteReport(report QueryReport) error {
	encoded, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	_, err = os.Stdout.Write(append(encoded, '\n'))
	return err
}

func ReportFor(engine string, query QuerySpec, input string, elapsed time.Duration, rows []RowReport, facets []FacetReport, unique []UniqueReport, counters map[string]uint64) QueryReport {
	if rows == nil {
		rows = []RowReport{}
	}
	if facets == nil {
		facets = []FacetReport{}
	}
	if unique == nil {
		unique = []UniqueReport{}
	}
	if counters == nil {
		counters = map[string]uint64{}
	}
	return QueryReport{
		Schema:         ReportSchema,
		Engine:         engine,
		Query:          query.Name,
		Input:          input,
		ElapsedSeconds: fmt.Sprintf("%.9f", elapsed.Seconds()),
		Rows:           rows,
		Facets:         facets,
		UniqueValues:   unique,
		Counters:       counters,
	}
}

func FieldReportFor(name, value []byte) FieldReport {
	return FieldReport{NameHex: EncodeHex(name), ValueHex: EncodeHex(value)}
}

func EncodeHex(value []byte) string {
	return hex.EncodeToString(value)
}
