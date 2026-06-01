package main

import (
	"flag"
	"fmt"
	"os"
	"time"

	contract "github.com/netdata/systemd-journal-sdk/go/internal/testcmd/explorer_query_contract"
	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func main() {
	input := flag.String("input", "", "journal file or directory")
	queryPath := flag.String("query", "", "query JSON path")
	surface := flag.String("surface", "file", "input surface: file or directory")
	flag.Parse()
	if *input == "" || *queryPath == "" {
		fmt.Fprintln(os.Stderr, "usage: explorer_query_optimized --input PATH --query PATH [--surface file|directory]")
		os.Exit(2)
	}

	query, err := contract.ReadQuery(*queryPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	start := time.Now()
	rows, facets, uniqueValues, counters, err := runOptimized(*input, *surface, query)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	report := contract.ReportFor("go-optimized-explorer-api", query, *input, time.Since(start), rows, facets, uniqueValues, counters)
	if err := contract.WriteReport(report); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func runOptimized(input, surface string, query contract.QuerySpec) ([]contract.RowReport, []contract.FacetReport, []contract.UniqueReport, map[string]uint64, error) {
	switch surface {
	case "file":
		reader, err := journal.OpenFileWithOptions(input, journal.DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			return nil, nil, nil, nil, err
		}
		defer reader.Close()
		return executeFile(reader, query)
	case "directory":
		reader, err := journal.OpenDirectoryWithOptions(input, journal.DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			return nil, nil, nil, nil, err
		}
		defer reader.Close()
		return executeDirectory(reader, query)
	default:
		return nil, nil, nil, nil, fmt.Errorf("unsupported --surface %q", surface)
	}
}

func executeFile(reader *journal.Reader, query contract.QuerySpec) ([]contract.RowReport, []contract.FacetReport, []contract.UniqueReport, map[string]uint64, error) {
	switch query.Mode {
	case contract.QueryModeQuery:
		explorer, err := explorerQuery(query)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		result, err := reader.ExplorerQuery(explorer)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		return rowReports(result.Rows), facetReports(result.Facets), nil, countersReport(result.Counters), nil
	case contract.QueryModeUnique:
		unique, err := explorerUniqueQuery(query)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		result, err := reader.ExplorerUnique(unique)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		return nil, nil, uniqueReports(result.Values), countersReport(result.Counters), nil
	default:
		return nil, nil, nil, nil, fmt.Errorf("unsupported query mode %q", query.Mode)
	}
}

func executeDirectory(reader *journal.DirectoryReader, query contract.QuerySpec) ([]contract.RowReport, []contract.FacetReport, []contract.UniqueReport, map[string]uint64, error) {
	switch query.Mode {
	case contract.QueryModeQuery:
		explorer, err := explorerQuery(query)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		result, err := reader.ExplorerQuery(explorer)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		return rowReports(result.Rows), facetReports(result.Facets), nil, countersReport(result.Counters), nil
	case contract.QueryModeUnique:
		unique, err := explorerUniqueQuery(query)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		result, err := reader.ExplorerUnique(unique)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		return nil, nil, uniqueReports(result.Values), countersReport(result.Counters), nil
	default:
		return nil, nil, nil, nil, fmt.Errorf("unsupported query mode %q", query.Mode)
	}
}

func explorerQuery(query contract.QuerySpec) (journal.ExplorerQuery, error) {
	filters, err := explorerFilters(query.Filters)
	if err != nil {
		return journal.ExplorerQuery{}, err
	}
	facets, err := valueSpecsBytes(query.Facets)
	if err != nil {
		return journal.ExplorerQuery{}, err
	}
	display, err := explorerDisplay(query)
	if err != nil {
		return journal.ExplorerQuery{}, err
	}
	out := journal.ExplorerQuery{
		Filters:           filters,
		Facets:            facets,
		Display:           display,
		Limit:             query.Limit,
		Direction:         explorerDirection(query.Direction),
		SinceRealtimeUsec: query.SinceRealtimeUsec,
		UntilRealtimeUsec: query.UntilRealtimeUsec,
	}
	if query.FullText != nil {
		needle, err := query.FullText.Bytes()
		if err != nil {
			return journal.ExplorerQuery{}, err
		}
		out = out.WithFullText(needle)
	}
	return out, nil
}

func explorerUniqueQuery(query contract.QuerySpec) (journal.ExplorerUniqueQuery, error) {
	if query.UniqueField == nil {
		return journal.ExplorerUniqueQuery{}, fmt.Errorf("unique mode requires unique_field")
	}
	field, err := query.UniqueField.Bytes()
	if err != nil {
		return journal.ExplorerUniqueQuery{}, err
	}
	filters, err := explorerFilters(query.Filters)
	if err != nil {
		return journal.ExplorerUniqueQuery{}, err
	}
	return journal.ExplorerUniqueQuery{
		Field:             field,
		Filters:           filters,
		Limit:             query.Limit,
		Skip:              query.UniqueSkip,
		IncludeCounts:     query.UniqueIncludeCounts,
		SinceRealtimeUsec: query.SinceRealtimeUsec,
		UntilRealtimeUsec: query.UntilRealtimeUsec,
	}, nil
}

func explorerFilters(filters []contract.FilterSpec) ([]journal.ExplorerFilter, error) {
	out := make([]journal.ExplorerFilter, 0, len(filters))
	for _, filter := range filters {
		field, err := filter.Field.Bytes()
		if err != nil {
			return nil, err
		}
		values, err := valueSpecsBytes(filter.Values)
		if err != nil {
			return nil, err
		}
		switch filter.Op {
		case contract.FilterOpIn:
			out = append(out, journal.FieldIn(field, values...))
		case contract.FilterOpNotIn:
			out = append(out, journal.FieldNotIn(field, values...))
		default:
			return nil, fmt.Errorf("unsupported filter op %q", filter.Op)
		}
	}
	return out, nil
}

func explorerDisplay(query contract.QuerySpec) (journal.ExplorerDisplay, error) {
	switch query.Display {
	case contract.DisplayNone:
		return journal.DisplayNone(), nil
	case contract.DisplayFields:
		fields, err := valueSpecsBytes(query.DisplayFields)
		if err != nil {
			return journal.ExplorerDisplay{}, err
		}
		return journal.DisplayFields(fields...), nil
	case contract.DisplayAll:
		return journal.DisplayAll(), nil
	default:
		return journal.ExplorerDisplay{}, fmt.Errorf("unsupported display mode %q", query.Display)
	}
}

func explorerDirection(direction contract.DirectionSpec) journal.Direction {
	if direction == contract.DirectionBackward {
		return journal.DirectionBackward
	}
	return journal.DirectionForward
}

func valueSpecsBytes(values []contract.ValueSpec) ([][]byte, error) {
	out := make([][]byte, 0, len(values))
	for _, value := range values {
		bytes, err := value.Bytes()
		if err != nil {
			return nil, err
		}
		out = append(out, bytes)
	}
	return out, nil
}

func rowReports(rows []journal.ExplorerRow) []contract.RowReport {
	out := make([]contract.RowReport, 0, len(rows))
	for _, row := range rows {
		fields := make([]contract.FieldReport, 0, len(row.Fields))
		for _, field := range row.Fields {
			fields = append(fields, contract.FieldReportFor(field.Name, field.Value))
		}
		out = append(out, contract.RowReport{
			Realtime: row.Realtime,
			Seqnum:   row.Seqnum,
			Cursor:   row.Cursor,
			Fields:   fields,
		})
	}
	return out
}

func facetReports(facets []journal.ExplorerFacet) []contract.FacetReport {
	out := make([]contract.FacetReport, 0, len(facets))
	for _, facet := range facets {
		values := make([]contract.FacetValueReport, 0, len(facet.Values))
		for _, value := range facet.Values {
			values = append(values, contract.FacetValueReport{
				ValueHex: contract.EncodeHex(value.Value),
				Count:    value.Count,
			})
		}
		out = append(out, contract.FacetReport{
			FieldHex: contract.EncodeHex(facet.Field),
			Values:   values,
		})
	}
	return out
}

func uniqueReports(values []journal.ExplorerUniqueValue) []contract.UniqueReport {
	out := make([]contract.UniqueReport, 0, len(values))
	for _, value := range values {
		out = append(out, contract.UniqueReport{
			ValueHex: contract.EncodeHex(value.Value),
			Count:    value.Count,
		})
	}
	return out
}

func countersReport(counters journal.ExplorerQueryCounters) map[string]uint64 {
	return map[string]uint64{
		"candidate_data_refs_visited":  counters.CandidateDataRefsVisited,
		"candidate_entries":            counters.CandidateEntries,
		"constrained_facet_counts":     counters.ConstrainedFacetCounts,
		"display_rows_expanded":        counters.DisplayRowsExpanded,
		"entry_offsets_indexed":        counters.EntryOffsetsIndexed,
		"facet_values_materialized":    counters.FacetValuesMaterialized,
		"field_linkage_fallbacks":      counters.FieldLinkageFallbacks,
		"field_linkage_hits":           counters.FieldLinkageHits,
		"filter_data_objects_examined": counters.FilterDataObjectsExamined,
		"fts_payloads_scanned":         counters.FTSPayloadsScanned,
		"payloads_decompressed":        counters.PayloadsDecompressed,
		"payloads_materialized":        counters.PayloadsMaterialized,
	}
}
