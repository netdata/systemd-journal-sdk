package main

import (
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const (
	baseRealtimeUsec  = uint64(1_700_000_000_000_000)
	baseMonotonicUsec = uint64(50_000_000)
	seqnumIDHex       = "22222222222222222222222222222222"
	fieldsPerRow      = 32
	defaultMaxSize    = 128 * 1024 * 1024
	fieldHashBuckets  = 1023
)

var (
	bootID    = mustUUID("0123456789abcdef0123456789abcdef")
	machineID = mustUUID("fedcba9876543210fedcba9876543210")
	seqnumID  = mustUUID(seqnumIDHex)
	fileID    = mustUUID("33333333333333333333333333333333")
)

type benchResult struct {
	Records                 int      `json:"records"`
	FieldsPerRow            int      `json:"fields_per_row"`
	AppendSeconds           float64  `json:"append_seconds"`
	AppendRowsPerSecond     float64  `json:"append_rows_per_second"`
	CloseSeconds            float64  `json:"close_seconds"`
	TotalWriterSeconds      float64  `json:"total_writer_seconds"`
	PrecomputeSeconds       float64  `json:"precompute_seconds"`
	JournalSizeBytes        int64    `json:"journal_size_bytes"`
	JournalPath             string   `json:"journal_path"`
	Format                  string   `json:"format"`
	Compression             string   `json:"compression"`
	FSS                     bool     `json:"fss"`
	APIMode                 string   `json:"api_mode"`
	DataHashBuckets         int      `json:"data_hash_table_buckets"`
	FieldHashBuckets        int      `json:"field_hash_table_buckets"`
	MaxSizeBytes            uint64   `json:"max_size_bytes"`
	LivePublishEveryEntries uint64   `json:"live_publish_every_entries"`
	AppendTimerExcludes     []string `json:"append_timer_excludes"`
	FinalState              string   `json:"final_state"`
	Errors                  []string `json:"errors"`
}

func mustUUID(s string) journal.UUID {
	b, err := hex.DecodeString(s)
	if err != nil || len(b) != 16 {
		panic("invalid UUID")
	}
	var id journal.UUID
	copy(id[:], b)
	return id
}

func bytesOf(s string) []byte {
	return []byte(s)
}

func dataHashBucketsForMaxSize(maxSize uint64) int {
	// Keep this driver aligned with journal.dataHashBucketsForMaxFileSize and
	// systemd's max_size * 4 / 768 / 3 formula.
	buckets := maxSize / 576
	if buckets < 2047 {
		buckets = 2047
	}
	if buckets > uint64(int(^uint(0)>>1)) {
		return int(^uint(0) >> 1)
	}
	return int(buckets)
}

func makeRows(rows int) [][]journal.Field {
	fixed := []journal.Field{
		{Name: "TEST_ID", Value: bytesOf("deterministic-ingestion-performance")},
		{Name: "PERF_PROFILE", Value: bytesOf("mixed-cardinality-32-fields")},
		{Name: "HOST_CLASS", Value: bytesOf("synthetic-edge")},
		{Name: "SOURCE_KIND", Value: bytesOf("journal-sdk-benchmark")},
	}
	lowValues := make([][][]byte, 12)
	for offset := range lowValues {
		lowValues[offset] = make([][]byte, 16)
		for value := range lowValues[offset] {
			lowValues[offset][value] = bytesOf(fmt.Sprintf("low-%02d-%02d", offset, value))
		}
	}
	mediumValues := make([][][]byte, 8)
	for offset := range mediumValues {
		mediumValues[offset] = make([][]byte, 2048)
		for value := range mediumValues[offset] {
			mediumValues[offset][value] = bytesOf(fmt.Sprintf("medium-%02d-%04d", offset, value))
		}
	}

	all := make([][]journal.Field, rows)
	for row := range rows {
		fields := make([]journal.Field, 0, fieldsPerRow)
		fields = append(fields, fixed...)
		for offset := 0; offset < 12; offset++ {
			fields = append(fields, journal.Field{
				Name:  fmt.Sprintf("LOW_CARD_%02d", offset),
				Value: lowValues[offset][row%16],
			})
		}
		for offset := 0; offset < 8; offset++ {
			fields = append(fields, journal.Field{
				Name:  fmt.Sprintf("MED_CARD_%02d", offset),
				Value: mediumValues[offset][row%2048],
			})
		}
		for offset := 0; offset < 8; offset++ {
			fields = append(fields, journal.Field{
				Name:  fmt.Sprintf("HIGH_CARD_%02d", offset),
				Value: bytesOf(fmt.Sprintf("high-%02d-%06d", offset, row)),
			})
		}
		all[row] = fields
	}
	return all
}

func archivePathFor(output string) string {
	prefix := strings.TrimSuffix(output, ".journal")
	return fmt.Sprintf("%s@%s-%016x-%016x.journal", prefix, seqnumIDHex, uint64(1), baseRealtimeUsec)
}

func closeWriter(w *journal.Writer, output string, finalState string) (string, error) {
	switch finalState {
	case "online":
		return output, w.Close()
	case "offline":
		return output, w.CloseOffline()
	case "archived":
		archivePath := archivePathFor(output)
		_ = os.Remove(archivePath)
		return archivePath, w.ArchiveTo(archivePath)
	default:
		return output, fmt.Errorf("invalid final state %q", finalState)
	}
}

func main() {
	var output string
	var format string
	var finalState string
	var maxSize uint64
	var livePublishEveryEntries uint64
	var rows int
	flag.StringVar(&output, "output", "", "journal output path")
	flag.StringVar(&format, "format", "compact", "journal format: compact or regular")
	flag.StringVar(&finalState, "final-state", "online", "final state: online, offline, or archived")
	flag.Uint64Var(&maxSize, "max-size-bytes", defaultMaxSize, "systemd max-size value used for hash table sizing")
	flag.Uint64Var(&livePublishEveryEntries, "live-publish-every-entries", 1, "explicit live-reader publication cadence; 0 disables explicit publication")
	flag.IntVar(&rows, "rows", 100_000, "number of rows")
	flag.Parse()

	dataHashBuckets := dataHashBucketsForMaxSize(maxSize)
	result := benchResult{
		Records:                 0,
		FieldsPerRow:            fieldsPerRow,
		Format:                  format,
		Compression:             "none",
		FSS:                     false,
		APIMode:                 "field-api",
		DataHashBuckets:         dataHashBuckets,
		FieldHashBuckets:        fieldHashBuckets,
		MaxSizeBytes:            maxSize,
		LivePublishEveryEntries: livePublishEveryEntries,
		AppendTimerExcludes:     []string{"row generation", "writer creation", "final close/sync", "journal verification"},
		FinalState:              finalState,
		Errors:                  []string{},
	}
	if output == "" {
		result.Errors = append(result.Errors, "--output is required")
		_ = json.NewEncoder(os.Stdout).Encode(result)
		os.Exit(2)
	}
	compact := format == "compact"
	if !compact && format != "regular" {
		result.Errors = append(result.Errors, "invalid --format")
		_ = json.NewEncoder(os.Stdout).Encode(result)
		os.Exit(2)
	}

	precomputeStart := time.Now()
	data := makeRows(rows)
	result.PrecomputeSeconds = time.Since(precomputeStart).Seconds()

	if err := os.MkdirAll(filepath.Dir(output), 0o755); err != nil {
		result.Errors = append(result.Errors, err.Error())
		_ = json.NewEncoder(os.Stdout).Encode(result)
		os.Exit(1)
	}
	_ = os.Remove(output)
	w, err := journal.Create(output, journal.Options{
		MachineID:               machineID,
		BootID:                  bootID,
		SeqnumID:                seqnumID,
		FileID:                  fileID,
		HeadSeqnum:              1,
		DataHashTableBuckets:    dataHashBuckets,
		FieldHashTableBuckets:   fieldHashBuckets,
		Compression:             journal.CompressionNone,
		CompressThresholdBytes:  512,
		Compact:                 compact,
		LivePublishEveryEntries: journal.PublishEveryEntries(livePublishEveryEntries),
	})
	if err != nil {
		result.Errors = append(result.Errors, err.Error())
		_ = json.NewEncoder(os.Stdout).Encode(result)
		os.Exit(1)
	}

	appendStart := time.Now()
	for index, fields := range data {
		if err := w.Append(fields, journal.EntryOptions{
			RealtimeUsec:     baseRealtimeUsec + uint64(index)*500,
			RealtimeUsecSet:  true,
			MonotonicUsec:    baseMonotonicUsec + uint64(index)*50,
			MonotonicUsecSet: true,
			BootID:           bootID,
		}); err != nil {
			result.Errors = append(result.Errors, err.Error())
			break
		}
		result.Records++
	}
	result.AppendSeconds = time.Since(appendStart).Seconds()
	if result.AppendSeconds > 0 {
		result.AppendRowsPerSecond = float64(result.Records) / result.AppendSeconds
	}

	closeStart := time.Now()
	journalPath, closeErr := closeWriter(w, output, finalState)
	result.CloseSeconds = time.Since(closeStart).Seconds()
	result.TotalWriterSeconds = result.AppendSeconds + result.CloseSeconds
	if closeErr != nil {
		result.Errors = append(result.Errors, closeErr.Error())
	}
	result.JournalPath = journalPath
	if stat, err := os.Stat(journalPath); err == nil {
		result.JournalSizeBytes = stat.Size()
	} else {
		result.Errors = append(result.Errors, err.Error())
	}

	_ = json.NewEncoder(os.Stdout).Encode(result)
	if len(result.Errors) > 0 || result.Records != rows {
		os.Exit(1)
	}
}
