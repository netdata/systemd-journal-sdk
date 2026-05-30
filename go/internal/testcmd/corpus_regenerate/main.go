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

func main() {
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
		fmt.Fprintln(os.Stderr, "--input and --output are required")
		os.Exit(2)
	}
	compact := false
	switch *format {
	case "regular":
		compact = false
	case "compact":
		compact = true
	default:
		fmt.Fprintf(os.Stderr, "invalid --format: %s\n", *format)
		os.Exit(2)
	}
	compression, err := parseCompression(*compressionName)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	opts := journal.DefaultReaderOptions().WithBounds(journal.ReaderBoundsSnapshot)
	reader, err := journal.OpenFileWithOptions(*input, opts)
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

	bootID := mustUUID(fallbackBoot)
	headSeqnum := uint64(1)
	fssStartUsec := uint64(1)
	if first != nil {
		bootID = first.BootID
		headSeqnum = first.Seqnum
		fssStartUsec = first.Realtime
	}
	if err := os.MkdirAll(filepath.Dir(*output), 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "create output directory: %v\n", err)
		os.Exit(1)
	}
	_ = os.Remove(*output)
	writerOptions := journal.Options{
		MachineID:               mustUUID("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		BootID:                  bootID,
		SeqnumID:                mustUUID("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		FileID:                  randomUUID(),
		HeadSeqnum:              headSeqnum,
		MaxFileSize:             *maxSize,
		DataHashTableBuckets:    dataHashBucketsForMaxSize(*maxSize),
		FieldHashTableBuckets:   1023,
		Compression:             compression,
		CompressThresholdBytes:  512,
		Compact:                 compact,
		LivePublishEveryEntries: journal.PublishEveryEntries(*livePublishEveryEntries),
		FieldNamePolicy:         journal.FieldNamePolicyRaw,
	}
	if *fss {
		writerOptions.Seal = &journal.SealOptions{
			Seed:         make([]byte, 12),
			IntervalUsec: *fssIntervalUsec,
			StartUsec:    fssStartUsec,
		}
	}
	w, err := journal.Create(*output, writerOptions)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create output: %v\n", err)
		os.Exit(1)
	}

	inputBytes := int64(0)
	if stat, err := os.Stat(*input); err == nil {
		inputBytes = stat.Size()
	}
	appendStarted := time.Now()
	var records, payloads, payloadBytes uint64
	appendEntry := func(entry *journal.Entry) error {
		err := w.AppendRaw(entry.Payloads, journal.EntryOptions{
			RealtimeUsec:     entry.Realtime,
			RealtimeUsecSet:  true,
			MonotonicUsec:    entry.Monotonic,
			MonotonicUsecSet: true,
			BootID:           entry.BootID,
		})
		if err != nil {
			return err
		}
		records++
		payloads += uint64(len(entry.Payloads))
		for _, payload := range entry.Payloads {
			payloadBytes += uint64(len(payload))
		}
		return nil
	}
	if first != nil {
		if err := appendEntry(first); err != nil {
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
		if err := appendEntry(entry); err != nil {
			fmt.Fprintf(os.Stderr, "append entry: %v\n", err)
			os.Exit(1)
		}
	}
	appendSeconds := time.Since(appendStarted).Seconds()
	closeStarted := time.Now()
	finalPath, closeErr := closeWriter(w, *output, *finalState)
	closeSeconds := time.Since(closeStarted).Seconds()
	if closeErr != nil {
		fmt.Fprintf(os.Stderr, "close output: %v\n", closeErr)
		os.Exit(1)
	}
	generatedBytes := int64(0)
	if stat, err := os.Stat(finalPath); err == nil {
		generatedBytes = stat.Size()
	}
	result := map[string]interface{}{
		"driver":                     "go",
		"records":                    records,
		"payloads":                   payloads,
		"payload_bytes":              payloadBytes,
		"input_bytes":                inputBytes,
		"generated_bytes":            generatedBytes,
		"generated_path":             finalPath,
		"format":                     *format,
		"compression":                *compressionName,
		"fss":                        *fss,
		"fss_start_usec":             nil,
		"fss_interval_usec":          nil,
		"final_state":                *finalState,
		"append_seconds":             appendSeconds,
		"close_seconds":              closeSeconds,
		"total_writer_seconds":       appendSeconds + closeSeconds,
		"live_publish_every_entries": *livePublishEveryEntries,
		"errors":                     []string{},
	}
	if *fss {
		result["fss_start_usec"] = fssStartUsec
		result["fss_interval_usec"] = *fssIntervalUsec
	}
	_ = json.NewEncoder(os.Stdout).Encode(result)
}
