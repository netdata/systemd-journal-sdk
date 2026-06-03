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
	Surface                 string   `json:"surface"`
	AppendSeconds           float64  `json:"append_seconds"`
	AppendRowsPerSecond     float64  `json:"append_rows_per_second"`
	CloseSeconds            float64  `json:"close_seconds"`
	TotalWriterSeconds      float64  `json:"total_writer_seconds"`
	PrecomputeSeconds       float64  `json:"precompute_seconds"`
	JournalSizeBytes        int64    `json:"journal_size_bytes"`
	JournalPath             string   `json:"journal_path"`
	JournalDirectory        string   `json:"journal_directory,omitempty"`
	JournalFiles            []string `json:"journal_files,omitempty"`
	Format                  string   `json:"format"`
	Compression             string   `json:"compression"`
	FSS                     bool     `json:"fss"`
	APIMode                 string   `json:"api_mode"`
	DataHashBuckets         int      `json:"data_hash_table_buckets"`
	FieldHashBuckets        int      `json:"field_hash_table_buckets"`
	MaxSizeBytes            uint64   `json:"max_size_bytes"`
	RotationMaxSizeBytes    uint64   `json:"rotation_max_size_bytes,omitempty"`
	LivePublication         string   `json:"live_publication"`
	LivePublishEveryEntries uint64   `json:"live_publish_every_entries"`
	AppendTimerExcludes     []string `json:"append_timer_excludes"`
	FinalState              string   `json:"final_state"`
	Errors                  []string `json:"errors"`
}

type writerConfig struct {
	output                  string
	format                  string
	finalState              string
	surface                 string
	maxSize                 uint64
	rotationMaxSize         uint64
	livePublishEveryEntries uint64
	apiMode                 string
	rows                    int
	compact                 bool
	dataHashBuckets         int
}

type benchRow struct {
	Fields   []journal.Field
	Payloads [][]byte
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

func makePayload(name string, value []byte) []byte {
	payload := make([]byte, 0, len(name)+1+len(value))
	payload = append(payload, name...)
	payload = append(payload, '=')
	payload = append(payload, value...)
	return payload
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

func livePublicationName(everyEntries uint64) string {
	switch everyEntries {
	case 0:
		return "disabled"
	case 1:
		return "immediate"
	default:
		return fmt.Sprintf("every-n:%d", everyEntries)
	}
}

func makeRows(rows int) []benchRow {
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

	all := make([]benchRow, rows)
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
		payloads := make([][]byte, 0, len(fields))
		for _, field := range fields {
			payloads = append(payloads, makePayload(field.Name, field.Value))
		}
		all[row] = benchRow{Fields: fields, Payloads: payloads}
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

func collectJournalFiles(root string) ([]string, int64, error) {
	files := []string{}
	var total int64
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".journal") {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		files = append(files, path)
		total += info.Size()
		return nil
	})
	return files, total, err
}

func runDirectory(
	result *benchResult,
	output string,
	format string,
	maxSize uint64,
	rotationMaxSize uint64,
	livePublishEveryEntries uint64,
	apiMode string,
	rows []benchRow,
) {
	compact := format == "compact"
	if err := os.RemoveAll(output); err != nil {
		result.Errors = append(result.Errors, err.Error())
		return
	}
	rotation := journal.RotationPolicy{}.WithMaxFileSize(rotationMaxSize)
	log, err := journal.NewLog(output, journal.LogConfig{
		Source: "system",
		Options: journal.Options{
			MachineID:               machineID,
			BootID:                  bootID,
			SeqnumID:                seqnumID,
			HeadSeqnum:              1,
			MaxFileSize:             maxSize,
			DataHashTableBuckets:    dataHashBucketsForMaxSize(maxSize),
			FieldHashTableBuckets:   fieldHashBuckets,
			Compression:             journal.CompressionNone,
			CompressThresholdBytes:  512,
			Compact:                 compact,
			LivePublishEveryEntries: journal.PublishEveryEntries(livePublishEveryEntries),
		},
		RotationPolicy: rotation,
		IdentityMode:   journal.LogIdentityStrict,
	})
	if err != nil {
		result.Errors = append(result.Errors, err.Error())
		return
	}

	appendStart := time.Now()
	for index, row := range rows {
		opts := journal.EntryOptions{
			RealtimeUsec:     baseRealtimeUsec + uint64(index)*500,
			RealtimeUsecSet:  true,
			MonotonicUsec:    baseMonotonicUsec + uint64(index)*50,
			MonotonicUsecSet: true,
			BootID:           bootID,
		}
		var err error
		if apiMode == "raw-payload" {
			err = log.AppendRaw(row.Payloads, opts)
		} else {
			err = log.Append(row.Fields, opts)
		}
		if err != nil {
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
	if err := log.Close(); err != nil {
		result.Errors = append(result.Errors, err.Error())
	}
	result.CloseSeconds = time.Since(closeStart).Seconds()
	result.TotalWriterSeconds = result.AppendSeconds + result.CloseSeconds
	result.JournalDirectory = log.JournalDirectory()
	result.JournalPath = result.JournalDirectory
	files, total, err := collectJournalFiles(output)
	if err != nil {
		result.Errors = append(result.Errors, err.Error())
		return
	}
	result.JournalFiles = files
	result.JournalSizeBytes = total
}

func parseWriterConfig() writerConfig {
	var output string
	var format string
	var finalState string
	var surface string
	var maxSize uint64
	var rotationMaxSize uint64
	var livePublishEveryEntries uint64
	var apiMode string
	var rows int
	flag.StringVar(&output, "output", "", "journal output path")
	flag.StringVar(&format, "format", "compact", "journal format: compact or regular")
	flag.StringVar(&finalState, "final-state", "online", "final state: online, offline, or archived")
	flag.StringVar(&surface, "surface", "direct", "writer surface: direct or directory")
	flag.Uint64Var(&maxSize, "max-size-bytes", defaultMaxSize, "systemd max-size value used for hash table sizing")
	flag.Uint64Var(&rotationMaxSize, "rotation-max-size-bytes", defaultMaxSize, "directory active-file rotation size")
	flag.Uint64Var(&livePublishEveryEntries, "live-publish-every-entries", 1, "explicit live-reader publication cadence; 0 disables explicit publication")
	flag.StringVar(&apiMode, "api-mode", "raw-payload", "append API shape: raw-payload or structured-field")
	flag.IntVar(&rows, "rows", 100_000, "number of rows")
	flag.Parse()

	dataHashBuckets := dataHashBucketsForMaxSize(maxSize)
	return writerConfig{
		output:                  output,
		format:                  format,
		finalState:              finalState,
		surface:                 surface,
		maxSize:                 maxSize,
		rotationMaxSize:         rotationMaxSize,
		livePublishEveryEntries: livePublishEveryEntries,
		apiMode:                 apiMode,
		rows:                    rows,
		compact:                 format == "compact",
		dataHashBuckets:         dataHashBuckets,
	}
}

func main() {
	cfg := parseWriterConfig()
	result := newBenchResult(cfg)
	if !validateWriterConfig(cfg, &result) {
		writeBenchResultAndExit(result, 2)
	}

	precomputeStart := time.Now()
	data := makeRows(cfg.rows)
	result.PrecomputeSeconds = time.Since(precomputeStart).Seconds()

	if cfg.surface == "directory" {
		runDirectory(&result, cfg.output, cfg.format, cfg.maxSize, cfg.rotationMaxSize, cfg.livePublishEveryEntries, cfg.apiMode, data)
		writeBenchResult(result, cfg.rows)
		return
	}

	runDirect(&result, cfg, data)
	writeBenchResult(result, cfg.rows)
}

func newBenchResult(cfg writerConfig) benchResult {
	return benchResult{
		Records:                 0,
		FieldsPerRow:            fieldsPerRow,
		Surface:                 cfg.surface,
		Format:                  cfg.format,
		Compression:             "none",
		FSS:                     false,
		APIMode:                 cfg.apiMode,
		DataHashBuckets:         cfg.dataHashBuckets,
		FieldHashBuckets:        fieldHashBuckets,
		MaxSizeBytes:            cfg.maxSize,
		RotationMaxSizeBytes:    cfg.rotationMaxSize,
		LivePublication:         livePublicationName(cfg.livePublishEveryEntries),
		LivePublishEveryEntries: cfg.livePublishEveryEntries,
		AppendTimerExcludes:     []string{"row generation", "writer creation", "final close/sync", "journal verification"},
		FinalState:              cfg.finalState,
		Errors:                  []string{},
	}
}

func validateWriterConfig(cfg writerConfig, result *benchResult) bool {
	if cfg.output == "" {
		result.Errors = append(result.Errors, "--output is required")
		return false
	}
	if !cfg.compact && cfg.format != "regular" {
		result.Errors = append(result.Errors, "invalid --format")
		return false
	}
	if cfg.apiMode != "raw-payload" && cfg.apiMode != "structured-field" {
		result.Errors = append(result.Errors, "invalid --api-mode")
		return false
	}
	if cfg.surface != "direct" && cfg.surface != "directory" {
		result.Errors = append(result.Errors, "invalid --surface")
		return false
	}
	return true
}

func runDirect(result *benchResult, cfg writerConfig, data []benchRow) {
	if err := os.MkdirAll(filepath.Dir(cfg.output), 0o755); err != nil {
		result.Errors = append(result.Errors, err.Error())
		return
	}
	_ = os.Remove(cfg.output)
	w, err := journal.Create(cfg.output, journal.Options{
		MachineID:               machineID,
		BootID:                  bootID,
		SeqnumID:                seqnumID,
		FileID:                  fileID,
		HeadSeqnum:              1,
		DataHashTableBuckets:    cfg.dataHashBuckets,
		FieldHashTableBuckets:   fieldHashBuckets,
		Compression:             journal.CompressionNone,
		CompressThresholdBytes:  512,
		Compact:                 cfg.compact,
		LivePublishEveryEntries: journal.PublishEveryEntries(cfg.livePublishEveryEntries),
	})
	if err != nil {
		result.Errors = append(result.Errors, err.Error())
		return
	}

	appendStart := time.Now()
	for index, row := range data {
		opts := journal.EntryOptions{
			RealtimeUsec:     baseRealtimeUsec + uint64(index)*500,
			RealtimeUsecSet:  true,
			MonotonicUsec:    baseMonotonicUsec + uint64(index)*50,
			MonotonicUsecSet: true,
			BootID:           bootID,
		}
		var err error
		if cfg.apiMode == "raw-payload" {
			err = w.AppendRaw(row.Payloads, opts)
		} else {
			err = w.Append(row.Fields, opts)
		}
		if err != nil {
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
	journalPath, closeErr := closeWriter(w, cfg.output, cfg.finalState)
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
}

func writeBenchResult(result benchResult, expectedRecords int) {
	_ = json.NewEncoder(os.Stdout).Encode(result)
	if len(result.Errors) > 0 || result.Records != expectedRecords {
		os.Exit(1)
	}
}

func writeBenchResultAndExit(result benchResult, code int) {
	_ = json.NewEncoder(os.Stdout).Encode(result)
	os.Exit(code)
}
