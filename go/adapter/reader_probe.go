package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type readerProbeRequest struct {
	Path      string `json:"path"`
	Operation string `json:"operation"`
	Field     string `json:"field"`
	MatchHex  string `json:"match_hex"`
	Realtime  uint64 `json:"realtime"`
	Snapshot  bool   `json:"snapshot"`
	Directory bool   `json:"directory"`
}

type probeRecovery struct {
	Reason           journal.RecoveryReason `json:"reason"`
	FirstLostOffset  uint64                 `json:"first_lost_offset"`
	DeclaredEntries  uint64                 `json:"declared_entries"`
	RecoveredEntries uint64                 `json:"recovered_entries"`
}

type probeFileRecovery struct {
	FileID string        `json:"file_id"`
	Status probeRecovery `json:"status"`
}

type readerProbeRow struct {
	Realtime uint64   `json:"realtime"`
	Payloads []string `json:"payloads"`
}

type readerProbeResult struct {
	Rows             []readerProbeRow    `json:"rows"`
	Fields           []string            `json:"fields"`
	Unique           []string            `json:"unique"`
	RecoveryStatuses []probeFileRecovery `json:"recovery_statuses"`
	Error            bool                `json:"error"`
	ErrorMessage     string              `json:"error_message,omitempty"`
}

type probeReader interface {
	Close() error
	Step() (bool, error)
	StepBack() (bool, error)
	SeekTail() error
	SeekRealtimeUsec(uint64) error
	GetRealtimeUsec() (uint64, error)
	CollectEntryPayloads() ([][]byte, error)
	EnumerateFields() (map[string]struct{}, error)
	VisitUnique(string, func([]byte) error) error
	QueryUniqueState(string) error
	EnumerateUniquePayload() ([]byte, bool, error)
	AddMatch([]byte)
	FlushMatches()
	RecoveryStatuses() []journal.FileRecoveryStatus
}

// This probe exposes public SDK results for shared damaged-file subprocess tests.
func runReaderProbe(input io.Reader, output io.Writer) error {
	var request readerProbeRequest
	if err := json.NewDecoder(input).Decode(&request); err != nil {
		return err
	}
	result := readerProbeResult{Rows: []readerProbeRow{}, Fields: []string{}, Unique: []string{}, RecoveryStatuses: []probeFileRecovery{}}
	err := inspectReader(request, &result)
	if err != nil {
		result.Error = true
		result.ErrorMessage = err.Error()
	}
	return json.NewEncoder(output).Encode(result)
}

func inspectReader(request readerProbeRequest, result *readerProbeResult) error {
	if request.Operation == "verify" {
		return journal.VerifyFile(request.Path)
	}
	if strings.HasPrefix(request.Operation, "facade-") {
		return inspectReaderFacade(request, result)
	}
	opts := journal.DefaultReaderOptions().WithSnapshot(request.Snapshot)
	var reader probeReader
	var err error
	if request.Directory {
		reader, err = journal.OpenDirectoryWithOptions(request.Path, opts)
	} else {
		reader, err = journal.OpenFileWithOptions(request.Path, opts)
	}
	if err != nil {
		return err
	}
	defer reader.Close()
	defer func() {
		recordProbeRecovery(result, reader.RecoveryStatuses())
	}()
	if request.MatchHex != "" {
		value, err := hex.DecodeString(request.MatchHex)
		if err != nil {
			return err
		}
		reader.AddMatch(value)
	}
	switch request.Operation {
	case "explorer", "explorer-filter", "explorer-facet", "explorer-histogram":
		file, ok := reader.(*journal.Reader)
		if !ok {
			return fmt.Errorf("explorer probe needs one file")
		}
		query := journal.DefaultExplorerQuery().WithFilter([]byte(request.Field), []byte("absent"))
		if request.Operation != "explorer" {
			query = journal.DefaultExplorerQuery()
			query.Limit = 0
			switch request.Operation {
			case "explorer-filter":
				query.Limit = 1
				query = query.WithFilter([]byte(request.Field), []byte("all"))
			case "explorer-facet":
				query = query.WithFacet([]byte(request.Field))
			case "explorer-histogram":
				query = query.WithHistogram([]byte(request.Field))
			}
		}
		query.FieldMode = journal.ExplorerFieldModeAllValues
		query.UseSourceRealtime = false
		_, err := file.ExploreWithStrategy(query, journal.ExplorerStrategyIndex)
		return err
	case "fields":
		fields, err := reader.EnumerateFields()
		if err != nil {
			return err
		}
		for field := range fields {
			result.Fields = append(result.Fields, field)
		}
		sort.Strings(result.Fields)
		return nil
	case "unique":
		return reader.VisitUnique(request.Field, func(value []byte) error { result.Unique = append(result.Unique, hex.EncodeToString(value)); return nil })
	case "unique-state":
		if err := reader.QueryUniqueState(request.Field); err != nil {
			return err
		}
		for {
			value, ok, err := reader.EnumerateUniquePayload()
			if err != nil {
				return err
			}
			if !ok {
				return nil
			}
			result.Unique = append(result.Unique, hex.EncodeToString(value))
		}
	case "flush":
		for i := 0; i < 2; i++ {
			if ok, err := reader.Step(); err != nil || !ok {
				if err != nil {
					return err
				}
				return io.ErrUnexpectedEOF
			}
		}
		reader.FlushMatches()
	case "reverse":
		if err := reader.SeekTail(); err != nil {
			return err
		}
	case "seek-forward", "seek-reverse":
		if err := reader.SeekRealtimeUsec(request.Realtime); err != nil {
			return err
		}
	case "forward":
	default:
		return fmt.Errorf("unsupported probe operation %q", request.Operation)
	}
	for {
		var ok bool
		if request.Operation == "reverse" || request.Operation == "seek-reverse" {
			ok, err = reader.StepBack()
		} else {
			ok, err = reader.Step()
		}
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}
		time, err := reader.GetRealtimeUsec()
		if err != nil {
			return err
		}
		payloads, err := reader.CollectEntryPayloads()
		if err != nil {
			return err
		}
		row := readerProbeRow{Realtime: time, Payloads: []string{}}
		for _, payload := range payloads {
			row.Payloads = append(row.Payloads, hex.EncodeToString(payload))
		}
		sort.Strings(row.Payloads)
		result.Rows = append(result.Rows, row)
	}
}

func recordProbeRecovery(result *readerProbeResult, statuses []journal.FileRecoveryStatus) {
	for _, status := range statuses {
		s := status.Status
		result.RecoveryStatuses = append(result.RecoveryStatuses, probeFileRecovery{status.FileID.String(), probeRecovery{s.Reason, s.FirstLostOffset, s.DeclaredEntries, s.RecoveredEntries}})
	}
}

func inspectReaderFacade(request readerProbeRequest, result *readerProbeResult) error {
	open := journal.SdJournalOpenFileWithOptions
	if request.Directory {
		open = journal.SdJournalOpenDirectoryWithOptions
	}
	reader, err := open(request.Path, 0, journal.DefaultReaderOptions().WithSnapshot(request.Snapshot))
	if err != nil {
		return err
	}
	defer journal.SdJournalClose(reader)
	defer func() { recordProbeRecovery(result, journal.SdJournalRecoveryStatuses(reader)) }()
	if request.MatchHex != "" {
		value, err := hex.DecodeString(request.MatchHex)
		if err != nil {
			return err
		}
		if err := journal.SdJournalAddMatch(reader, value); err != nil {
			return err
		}
	}
	operation := strings.TrimPrefix(request.Operation, "facade-")
	switch operation {
	case "fields":
		fields, err := journal.SdJournalEnumerateFields(reader)
		if err != nil {
			return err
		}
		result.Fields = append(result.Fields, fields...)
		sort.Strings(result.Fields)
		return err
	case "unique":
		values, err := journal.SdJournalQueryUnique(reader, request.Field)
		if err != nil {
			return err
		}
		for _, value := range values {
			result.Unique = append(result.Unique, hex.EncodeToString(value.Value))
		}
		return nil
	case "unique-state":
		if err := journal.SdJournalQueryUniqueState(reader, request.Field); err != nil {
			return err
		}
		for {
			payload, ok, err := journal.SdJournalEnumerateAvailableUnique(reader)
			if err != nil || !ok {
				return err
			}
			result.Unique = append(result.Unique, hex.EncodeToString(payload))
		}
	case "flush":
		for i := 0; i < 2; i++ {
			if count, err := journal.SdJournalNext(reader); err != nil || count != 1 {
				if err != nil {
					return err
				}
				return io.ErrUnexpectedEOF
			}
		}
		if err := journal.SdJournalFlushMatches(reader); err != nil {
			return err
		}
	case "forward", "reverse":
		if operation == "reverse" {
			if err := journal.SdJournalSeekTail(reader); err != nil {
				return err
			}
		}
	default:
		return fmt.Errorf("unknown facade probe operation %q", operation)
	}
	for {
		step := journal.SdJournalNext
		if operation == "reverse" {
			step = journal.SdJournalPrevious
		}
		count, err := step(reader)
		if err != nil || count == 0 {
			return err
		}
		entry, err := journal.SdJournalGetEntry(reader)
		if err != nil {
			return err
		}
		row := readerProbeRow{Realtime: entry.Realtime, Payloads: make([]string, 0, len(entry.Payloads))}
		for _, payload := range entry.Payloads {
			row.Payloads = append(row.Payloads, hex.EncodeToString(payload))
		}
		sort.Strings(row.Payloads)
		result.Rows = append(result.Rows, row)
	}
}
