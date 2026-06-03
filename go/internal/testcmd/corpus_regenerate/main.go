package main

import (
	"crypto/rand"
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
	defaultMaxSize = uint64(128 * 1024 * 1024)
	fallbackBoot   = "dddddddddddddddddddddddddddddddd"
)

func mustUUID(s string) journal.UUID {
	raw, err := hex.DecodeString(s)
	if err != nil || len(raw) != 16 {
		panic("invalid UUID")
	}
	var id journal.UUID
	copy(id[:], raw)
	return id
}

func randomUUID() journal.UUID {
	var id journal.UUID
	if _, err := rand.Read(id[:]); err != nil {
		return mustUUID("cccccccccccccccccccccccccccccccc")
	}
	return id
}

func dataHashBucketsForMaxSize(maxSize uint64) int {
	buckets := maxSize / 576
	if buckets < 2047 {
		buckets = 2047
	}
	if buckets > uint64(int(^uint(0)>>1)) {
		return int(^uint(0) >> 1)
	}
	return int(buckets)
}

func systemdFSSStartUsec(realtime, interval uint64) uint64 {
	if interval == 0 {
		return realtime
	}
	return (realtime / interval) * interval
}

func parseCompression(value string) (int, error) {
	switch value {
	case "none":
		return journal.CompressionNone, nil
	case "zstd":
		return journal.CompressionZSTD, nil
	case "xz":
		return journal.CompressionXZ, nil
	case "lz4":
		return journal.CompressionLZ4, nil
	default:
		return journal.CompressionNone, fmt.Errorf("invalid --compression: %s", value)
	}
}

func closeWriter(w *journal.Writer, output string, finalState string) (string, error) {
	switch finalState {
	case "online":
		return output, w.Close()
	case "offline":
		return output, w.CloseOffline()
	case "archived":
		archivePath := strings.TrimSuffix(output, ".journal") + "@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-0000000000000001-0000000000000001.journal"
		_ = os.Remove(archivePath)
		return archivePath, w.ArchiveTo(archivePath)
	default:
		return output, fmt.Errorf("invalid --final-state: %s", finalState)
	}
}

type regenerateConfig struct {
	input                   string
	output                  string
	format                  string
	compressionName         string
	compression             int
	fss                     bool
	fssIntervalUsec         uint64
	finalState              string
	maxSize                 uint64
	livePublishEveryEntries uint64
	compact                 bool
}

type regenerateStats struct {
	records      uint64
	payloads     uint64
	payloadBytes uint64
}

func parseRegenerateConfig() (regenerateConfig, error) {
	input := flag.String("input", "", "journal input file")
	output := flag.String("output", "", "journal output file")
	format := flag.String("format", "regular", "journal format: regular or compact")
	compressionName := flag.String("compression", "none", "compression: none, zstd, xz, or lz4")
	fss := flag.Bool("fss", false, "enable deterministic synthetic Forward Secure Sealing")
	fssIntervalUsec := flag.Uint64("fss-interval-usec", 1_000_000, "FSS interval")
	finalState := flag.String("final-state", "offline", "final state: online, offline, archived")
	maxSize := flag.Uint64("max-size-bytes", defaultMaxSize, "hash-table sizing max file size")
	livePublishEveryEntries := flag.Uint64("live-publish-every-entries", 1, "live publication cadence")
	flag.Parse()
	if *input == "" || *output == "" {
		return regenerateConfig{}, fmt.Errorf("--input and --output are required")
	}
	compact := false
	switch *format {
	case "regular":
		compact = false
	case "compact":
		compact = true
	default:
		return regenerateConfig{}, fmt.Errorf("invalid --format: %s", *format)
	}
	compression, err := parseCompression(*compressionName)
	if err != nil {
		return regenerateConfig{}, err
	}
	return regenerateConfig{
		input:                   *input,
		output:                  *output,
		format:                  *format,
		compressionName:         *compressionName,
		compression:             compression,
		fss:                     *fss,
		fssIntervalUsec:         *fssIntervalUsec,
		finalState:              *finalState,
		maxSize:                 *maxSize,
		livePublishEveryEntries: *livePublishEveryEntries,
		compact:                 compact,
	}, nil
}

func main() {
	cfg, err := parseRegenerateConfig()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	reader, first := openInputReader(cfg.input)
	defer reader.Close()
	bootID, headSeqnum, fssStartUsec := firstEntryMetadata(first, cfg.fssIntervalUsec)
	w := createOutputWriter(cfg, bootID, headSeqnum, fssStartUsec)
	appendSeconds, stats := copyEntries(reader, w, first)
	closeStarted := time.Now()
	finalPath, closeErr := closeWriter(w, cfg.output, cfg.finalState)
	closeSeconds := time.Since(closeStarted).Seconds()
	if closeErr != nil {
		fmt.Fprintf(os.Stderr, "close output: %v\n", closeErr)
		os.Exit(1)
	}
	_ = json.NewEncoder(os.Stdout).Encode(regenerateOutput(cfg, stats, appendSeconds, closeSeconds, finalPath, fssStartUsec))
}

func openInputReader(input string) (*journal.Reader, *journal.Entry) {
	opts := journal.DefaultReaderOptions().WithBounds(journal.ReaderBoundsSnapshot)
	reader, err := journal.OpenFileWithOptions(input, opts)
	if err != nil {
		fmt.Fprintf(os.Stderr, "open input: %v\n", err)
		os.Exit(1)
	}
	defer reader.Close()
	if err := reader.SeekHead(); err != nil {
		fmt.Fprintf(os.Stderr, "seek head: %v\n", err)
		os.Exit(1)
	}
	var first *journal.Entry
	if ok, err := reader.Step(); err != nil {
		fmt.Fprintf(os.Stderr, "read first: %v\n", err)
		os.Exit(1)
	} else if ok {
		first, err = reader.GetEntry()
		if err != nil {
			fmt.Fprintf(os.Stderr, "get first: %v\n", err)
			os.Exit(1)
		}
	}
	return reader, first
}

func firstEntryMetadata(first *journal.Entry, interval uint64) (journal.UUID, uint64, uint64) {
	bootID := mustUUID(fallbackBoot)
	headSeqnum := uint64(1)
	fssStartUsec := interval
	if first != nil {
		bootID = first.BootID
		headSeqnum = first.Seqnum
		fssStartUsec = systemdFSSStartUsec(first.Realtime, interval)
	}
	return bootID, headSeqnum, fssStartUsec
}

func createOutputWriter(cfg regenerateConfig, bootID journal.UUID, headSeqnum uint64, fssStartUsec uint64) *journal.Writer {
	if err := os.MkdirAll(filepath.Dir(cfg.output), 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "create output directory: %v\n", err)
		os.Exit(1)
	}
	_ = os.Remove(cfg.output)
	writerOptions := journal.Options{
		MachineID:               mustUUID("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		BootID:                  bootID,
		SeqnumID:                mustUUID("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		FileID:                  randomUUID(),
		HeadSeqnum:              headSeqnum,
		MaxFileSize:             cfg.maxSize,
		DataHashTableBuckets:    dataHashBucketsForMaxSize(cfg.maxSize),
		FieldHashTableBuckets:   1023,
		Compression:             cfg.compression,
		CompressThresholdBytes:  512,
		Compact:                 cfg.compact,
		LivePublishEveryEntries: journal.PublishEveryEntries(cfg.livePublishEveryEntries),
		FieldNamePolicy:         journal.FieldNamePolicyRaw,
	}
	if cfg.fss {
		writerOptions.Seal = &journal.SealOptions{
			Seed:         make([]byte, 12),
			IntervalUsec: cfg.fssIntervalUsec,
			StartUsec:    fssStartUsec,
		}
	}
	w, err := journal.Create(cfg.output, writerOptions)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create output: %v\n", err)
		os.Exit(1)
	}
	return w
}

func copyEntries(reader *journal.Reader, w *journal.Writer, first *journal.Entry) (float64, regenerateStats) {
	appendStarted := time.Now()
	var stats regenerateStats
	if first != nil {
		if err := appendEntry(w, first, &stats); err != nil {
			fmt.Fprintf(os.Stderr, "append first: %v\n", err)
			os.Exit(1)
		}
	}
	for {
		ok, err := reader.Step()
		if err != nil {
			fmt.Fprintf(os.Stderr, "read entry: %v\n", err)
			os.Exit(1)
		}
		if !ok {
			break
		}
		entry, err := reader.GetEntry()
		if err != nil {
			fmt.Fprintf(os.Stderr, "get entry: %v\n", err)
			os.Exit(1)
		}
		if err := appendEntry(w, entry, &stats); err != nil {
			fmt.Fprintf(os.Stderr, "append entry: %v\n", err)
			os.Exit(1)
		}
	}
	return time.Since(appendStarted).Seconds(), stats
}

func appendEntry(w *journal.Writer, entry *journal.Entry, stats *regenerateStats) error {
	err := w.AppendRaw(entry.Payloads, journal.EntryOptions{
		RealtimeUsec:     entry.Realtime,
		RealtimeUsecSet:  true,
		MonotonicUsec:    entry.Monotonic,
		MonotonicUsecSet: true,
		BootID:           entry.BootID,
		Seqnum:           entry.Seqnum,
	})
	if err != nil {
		return err
	}
	stats.records++
	stats.payloads += uint64(len(entry.Payloads))
	for _, payload := range entry.Payloads {
		stats.payloadBytes += uint64(len(payload))
	}
	return nil
}

func regenerateOutput(
	cfg regenerateConfig,
	stats regenerateStats,
	appendSeconds float64,
	closeSeconds float64,
	finalPath string,
	fssStartUsec uint64,
) map[string]interface{} {
	result := map[string]interface{}{
		"driver":                     "go",
		"records":                    stats.records,
		"payloads":                   stats.payloads,
		"payload_bytes":              stats.payloadBytes,
		"input_bytes":                fileSize(cfg.input),
		"generated_bytes":            fileSize(finalPath),
		"generated_path":             finalPath,
		"format":                     cfg.format,
		"compression":                cfg.compressionName,
		"fss":                        cfg.fss,
		"fss_start_usec":             nil,
		"fss_interval_usec":          nil,
		"final_state":                cfg.finalState,
		"append_seconds":             appendSeconds,
		"close_seconds":              closeSeconds,
		"total_writer_seconds":       appendSeconds + closeSeconds,
		"live_publish_every_entries": cfg.livePublishEveryEntries,
		"errors":                     []string{},
	}
	if cfg.fss {
		result["fss_start_usec"] = fssStartUsec
		result["fss_interval_usec"] = cfg.fssIntervalUsec
	}
	return result
}

func fileSize(path string) int64 {
	if stat, err := os.Stat(path); err == nil {
		return stat.Size()
	}
	return 0
}
