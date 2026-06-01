package main

import (
	"bytes"
	"flag"
	"fmt"
	"os"
	"sort"
	"time"

	contract "github.com/netdata/systemd-journal-sdk/go/internal/testcmd/explorer_query_contract"
	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type existingReader interface {
	SeekHead() error
	SeekTail() error
	Step() (bool, error)
	StepBack() (bool, error)
	GetEntry() (*journal.Entry, error)
}

func main() {
	input := flag.String("input", "", "journal file or directory")
	queryPath := flag.String("query", "", "query JSON path")
	surface := flag.String("surface", "file", "input surface: file or directory")
	flag.Parse()
	if *input == "" || *queryPath == "" {
		fmt.Fprintln(os.Stderr, "usage: explorer_query_baseline --input PATH --query PATH [--surface file|directory]")
		os.Exit(2)
	}

	query, err := contract.ReadQuery(*queryPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	start := time.Now()
	rows, facets, uniqueValues, counters, err := runBaseline(*input, *surface, query)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	report := contract.ReportFor("go-baseline-existing-api", query, *input, time.Since(start), rows, facets, uniqueValues, counters)
	if err := contract.WriteReport(report); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func runBaseline(input, surface string, query contract.QuerySpec) ([]contract.RowReport, []contract.FacetReport, []contract.UniqueReport, map[string]uint64, error) {
	switch surface {
	case "file":
		reader, err := journal.OpenFileWithOptions(input, journal.DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			return nil, nil, nil, nil, err
		}
		defer reader.Close()
		return executeReader(reader, query)
	case "directory":
		reader, err := journal.OpenDirectoryWithOptions(input, journal.DefaultReaderOptions().WithSnapshot(true))
		if err != nil {
			return nil, nil, nil, nil, err
		}
		defer reader.Close()
		return executeReader(reader, query)
	default:
		return nil, nil, nil, nil, fmt.Errorf("unsupported --surface %q", surface)
	}
}

func executeReader(reader existingReader, query contract.QuerySpec) ([]contract.RowReport, []contract.FacetReport, []contract.UniqueReport, map[string]uint64, error) {
	if query.Direction == contract.DirectionBackward {
		if err := reader.SeekTail(); err != nil {
			return nil, nil, nil, nil, err
		}
	} else if err := reader.SeekHead(); err != nil {
		return nil, nil, nil, nil, err
	}

	var rows []contract.RowReport
	facetMaps := make(map[string]map[string]facetBucket)
	uniqueMap := make(map[string]uniqueBucket)
	counters := make(map[string]uint64)

	var uniqueField []byte
	var err error
	if query.UniqueField != nil {
		uniqueField, err = query.UniqueField.Bytes()
		if err != nil {
			return nil, nil, nil, nil, err
		}
	}
	var needle []byte
	if query.FullText != nil {
		needle, err = query.FullText.Bytes()
		if err != nil {
			return nil, nil, nil, nil, err
		}
	}

	for {
		var ok bool
		if query.Direction == contract.DirectionBackward {
			ok, err = reader.StepBack()
		} else {
			ok, err = reader.Step()
		}
		if err != nil {
			return nil, nil, nil, nil, err
		}
		if !ok {
			break
		}
		counters["entries_read"]++
		entry, err := reader.GetEntry()
		if err != nil {
			return nil, nil, nil, nil, err
		}
		counters["entries_expanded"]++
		counters["payloads_seen"] += uint64(len(entry.Payloads))

		if !timeMatches(query, entry.Realtime) {
			continue
		}
		matched, err := entryMatchesFilters(entry, query)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		if !matched {
			continue
		}
		if query.FullText != nil {
			if !entryMatchesFullText(entry, needle) {
				continue
			}
			counters["fts_payloads_scanned"] += uint64(len(entry.Payloads))
		}

		for _, facet := range query.Facets {
			field, err := facet.Bytes()
			if err != nil {
				return nil, nil, nil, nil, err
			}
			for _, value := range entry.RawValues(field) {
				addFacetValue(facetMaps, field, value)
			}
		}

		if query.Mode == contract.QueryModeUnique {
			if uniqueField != nil {
				for _, value := range entry.RawValues(uniqueField) {
					addUniqueValue(uniqueMap, value)
				}
			}
			continue
		}

		if query.Limit == nil || len(rows) < *query.Limit {
			row, err := rowReport(entry, query)
			if err != nil {
				return nil, nil, nil, nil, err
			}
			rows = append(rows, row)
		}
	}

	return rows, facetReports(facetMaps), uniqueReports(uniqueMap, query), counters, nil
}

func entryMatchesFilters(entry *journal.Entry, query contract.QuerySpec) (bool, error) {
	for _, filter := range query.Filters {
		field, err := filter.Field.Bytes()
		if err != nil {
			return false, err
		}
		values, err := filterValues(filter)
		if err != nil {
			return false, err
		}
		matched := false
		for _, want := range values {
			for _, got := range entry.RawValues(field) {
				if bytes.Equal(got, want) {
					matched = true
					break
				}
			}
			if matched {
				break
			}
		}
		switch filter.Op {
		case contract.FilterOpIn:
			if !matched {
				return false, nil
			}
		case contract.FilterOpNotIn:
			if matched {
				return false, nil
			}
		default:
			return false, fmt.Errorf("unsupported filter op %q", filter.Op)
		}
	}
	return true, nil
}

func filterValues(filter contract.FilterSpec) ([][]byte, error) {
	values := make([][]byte, 0, len(filter.Values))
	for _, value := range filter.Values {
		bytes, err := value.Bytes()
		if err != nil {
			return nil, err
		}
		values = append(values, bytes)
	}
	return values, nil
}

func entryMatchesFullText(entry *journal.Entry, needle []byte) bool {
	if len(needle) == 0 {
		return true
	}
	for _, payload := range entry.Payloads {
		if bytes.Contains(payload, needle) {
			return true
		}
	}
	return false
}

func rowReport(entry *journal.Entry, query contract.QuerySpec) (contract.RowReport, error) {
	fields, err := rowFields(entry, query)
	if err != nil {
		return contract.RowReport{}, err
	}
	return contract.RowReport{
		Realtime: entry.Realtime,
		Seqnum:   entry.Seqnum,
		Cursor:   entry.Cursor,
		Fields:   fields,
	}, nil
}

func rowFields(entry *journal.Entry, query contract.QuerySpec) ([]contract.FieldReport, error) {
	switch query.Display {
	case contract.DisplayNone:
		return []contract.FieldReport{}, nil
	case contract.DisplayAll:
		fields := make([]contract.FieldReport, 0, len(entry.RawFields))
		for _, field := range entry.RawFields {
			fields = append(fields, contract.FieldReportFor(field.Name, field.Value))
		}
		return fields, nil
	case contract.DisplayFields:
		selected := make([][]byte, 0, len(query.DisplayFields))
		for _, field := range query.DisplayFields {
			value, err := field.Bytes()
			if err != nil {
				return nil, err
			}
			selected = append(selected, value)
		}
		var fields []contract.FieldReport
		for _, field := range entry.RawFields {
			for _, want := range selected {
				if bytes.Equal(field.Name, want) {
					fields = append(fields, contract.FieldReportFor(field.Name, field.Value))
					break
				}
			}
		}
		return fields, nil
	default:
		return nil, fmt.Errorf("unsupported display mode %q", query.Display)
	}
}

func timeMatches(query contract.QuerySpec, realtime uint64) bool {
	if query.SinceRealtimeUsec != nil && realtime < *query.SinceRealtimeUsec {
		return false
	}
	if query.UntilRealtimeUsec != nil && realtime >= *query.UntilRealtimeUsec {
		return false
	}
	return true
}

type facetBucket struct {
	value []byte
	count uint64
}

func addFacetValue(facetMaps map[string]map[string]facetBucket, field, value []byte) {
	fieldKey := string(field)
	values := facetMaps[fieldKey]
	if values == nil {
		values = make(map[string]facetBucket)
		facetMaps[fieldKey] = values
	}
	valueKey := string(value)
	bucket := values[valueKey]
	if bucket.value == nil {
		bucket.value = append([]byte(nil), value...)
	}
	bucket.count++
	values[valueKey] = bucket
}

func facetReports(facetMaps map[string]map[string]facetBucket) []contract.FacetReport {
	facets := make([]contract.FacetReport, 0, len(facetMaps))
	for fieldKey, valuesMap := range facetMaps {
		values := make([]contract.FacetValueReport, 0, len(valuesMap))
		for _, bucket := range valuesMap {
			values = append(values, contract.FacetValueReport{
				ValueHex: contract.EncodeHex(bucket.value),
				Count:    bucket.count,
			})
		}
		sort.Slice(values, func(i, j int) bool {
			return values[i].ValueHex < values[j].ValueHex
		})
		facets = append(facets, contract.FacetReport{
			FieldHex: contract.EncodeHex([]byte(fieldKey)),
			Values:   values,
		})
	}
	sort.Slice(facets, func(i, j int) bool {
		return facets[i].FieldHex < facets[j].FieldHex
	})
	return facets
}

type uniqueBucket struct {
	value []byte
	count uint64
}

func addUniqueValue(uniqueMap map[string]uniqueBucket, value []byte) {
	key := string(value)
	bucket := uniqueMap[key]
	if bucket.value == nil {
		bucket.value = append([]byte(nil), value...)
	}
	bucket.count++
	uniqueMap[key] = bucket
}

func uniqueReports(uniqueMap map[string]uniqueBucket, query contract.QuerySpec) []contract.UniqueReport {
	values := make([]contract.UniqueReport, 0, len(uniqueMap))
	for _, bucket := range uniqueMap {
		var count *uint64
		if query.UniqueIncludeCounts {
			c := bucket.count
			count = &c
		}
		values = append(values, contract.UniqueReport{
			ValueHex: contract.EncodeHex(bucket.value),
			Count:    count,
		})
	}
	sort.Slice(values, func(i, j int) bool {
		return values[i].ValueHex < values[j].ValueHex
	})
	start := query.UniqueSkip
	if start < 0 {
		start = 0
	}
	if start > len(values) {
		return nil
	}
	end := len(values)
	if query.Limit != nil && start+*query.Limit < end {
		end = start + *query.Limit
	}
	return values[start:end]
}
