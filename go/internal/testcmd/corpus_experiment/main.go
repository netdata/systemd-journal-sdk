package main

import (
	"bufio"
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const (
	rawReaderSchema = "systemd-journal-sdk-raw-reader-v1"
	rawReaderMagic  = "systemd-journal-sdk-raw-reader-v1\x00"
	spoolSchema     = "systemd-journal-sdk-spool-v1"
	defaultMaxSize  = uint64(128 * 1024 * 1024)
)

type rawCounts struct {
	Entries                  uint64 `json:"entries"`
	Payloads                 uint64 `json:"payloads"`
	PayloadBytes             uint64 `json:"payload_bytes"`
	BinaryPayloads           uint64 `json:"binary_payloads"`
	PayloadsWithoutSeparator uint64 `json:"payloads_without_separator"`
	LargestPayloadBytes      uint64 `json:"largest_payload_bytes"`
}

type spoolEntry struct {
	Realtime  uint64
	Monotonic uint64
	Seqnum    uint64
	BootID    journal.UUID
	Payloads  [][]byte
}

func main() {
	if len(os.Args) < 2 {
		usageAndExit()
	}
	switch os.Args[1] {
	case "raw-read":
		rawRead(os.Args[2:])
	case "dump-spool":
		dumpSpool(os.Args[2:])
	case "write-spool":
		writeSpool(os.Args[2:])
	default:
		usageAndExit()
	}
}

func usageAndExit() {
	fmt.Fprintln(os.Stderr, "usage: corpus_experiment <raw-read|dump-spool|write-spool> [options]")
	os.Exit(2)
}

func rawRead(args []string) {
	fs := flag.NewFlagSet("raw-read", flag.ExitOnError)
	input := fs.String("input", "", "journal file to read")
	directory := fs.String("directory", "", "directory to discover internally")
	limit := fs.Int("limit-files", 0, "maximum discovered files to read")
	output := fs.String("output", "csv", "output format: csv or json")
	access := fs.String("access", "mmap", "reader access: mmap or read-at")
	_ = fs.Parse(args)

	paths, err := inputPaths(*input, *directory, *limit)
	if err != nil {
		exitError(err)
	}
	rows := make([]map[string]any, 0, len(paths))
	for _, path := range paths {
		row := rawReadOne(path, *access)
		rows = append(rows, row)
	}
	if *output == "json" {
		_ = json.NewEncoder(os.Stdout).Encode(rows)
		return
	}
	writeRawCSV(rows)
}

func rawReadOne(path, access string) map[string]any {
	started := time.Now()
	opts := journal.DefaultReaderOptions().WithBounds(journal.ReaderBoundsSnapshot)
	switch access {
	case "mmap":
		opts = opts.WithAccessMode(journal.ReaderAccessMmap)
	case "read-at":
		opts = opts.WithAccessMode(journal.ReaderAccessReadAt)
	default:
		return errorRow(path, "invalid_access", fmt.Sprintf("invalid access %q", access))
	}
	reader, err := journal.OpenFileWithOptions(path, opts)
	if err != nil {
		return errorRow(path, "open", err.Error())
	}
	defer reader.Close()
	if err := reader.SeekHead(); err != nil {
		return errorRow(path, "seek", err.Error())
	}
	hash := sha256.New()
	_, _ = hash.Write([]byte(rawReaderMagic))
	var counts rawCounts
	var lenBuf [8]byte
	for {
		ok, err := reader.Step()
		if err != nil {
			return errorRow(path, "step", err.Error())
		}
		if !ok {
			break
		}
		_, _ = hash.Write([]byte("E"))
		binary.BigEndian.PutUint64(lenBuf[:], counts.Entries)
		_, _ = hash.Write(lenBuf[:])
		err = reader.VisitEntryPayloads(func(payload []byte) error {
			_, _ = hash.Write([]byte("P"))
			binary.BigEndian.PutUint64(lenBuf[:], uint64(len(payload)))
			_, _ = hash.Write(lenBuf[:])
			_, _ = hash.Write(payload)
			counts.Payloads++
			counts.PayloadBytes += uint64(len(payload))
			if uint64(len(payload)) > counts.LargestPayloadBytes {
				counts.LargestPayloadBytes = uint64(len(payload))
			}
			if payloadName(payload) == nil {
				counts.PayloadsWithoutSeparator++
			}
			if payloadHasBinary(payload) {
				counts.BinaryPayloads++
			}
			return nil
		})
		if err != nil {
			return errorRow(path, "payload", err.Error())
		}
		_, _ = hash.Write([]byte("e"))
		counts.Entries++
	}
	elapsed := time.Since(started).Seconds()
	inputBytes := fileSize(path)
	return map[string]any{
		"schema":                   rawReaderSchema,
		"driver":                   "go",
		"status":                   "ok",
		"file_id":                  sanitizedFileID(path),
		"input_bytes":              inputBytes,
		"entries":                  counts.Entries,
		"payloads":                 counts.Payloads,
		"payload_bytes":            counts.PayloadBytes,
		"binary_payloads":          counts.BinaryPayloads,
		"payloads_without_equals":  counts.PayloadsWithoutSeparator,
		"largest_payload_bytes":    counts.LargestPayloadBytes,
		"hash":                     hex.EncodeToString(hash.Sum(nil)),
		"elapsed_seconds":          elapsed,
		"entries_per_second":       rate(counts.Entries, elapsed),
		"payloads_per_second":      rate(counts.Payloads, elapsed),
		"payload_bytes_per_second": rate(counts.PayloadBytes, elapsed),
		"input_bytes_per_second":   rate(uint64(inputBytes), elapsed),
		"reader_path":              "raw-payload-visitor",
	}
}

func dumpSpool(args []string) {
	fs := flag.NewFlagSet("dump-spool", flag.ExitOnError)
	input := fs.String("input", "", "journal file to dump")
	output := fs.String("output", "-", "spool output path or - for stdout")
	_ = fs.Parse(args)
	if *input == "" {
		exitError(errors.New("--input is required"))
	}
	reader, err := journal.OpenFileWithOptions(*input, journal.DefaultReaderOptions().WithBounds(journal.ReaderBoundsSnapshot))
	if err != nil {
		exitError(err)
	}
	defer reader.Close()
	if err := reader.SeekHead(); err != nil {
		exitError(err)
	}
	var out io.Writer = os.Stdout
	var file *os.File
	if *output != "-" {
		file, err = os.Create(*output)
		if err != nil {
			exitError(err)
		}
		defer file.Close()
		out = file
	}
	buffered := bufio.NewWriter(out)
	defer buffered.Flush()
	for {
		ok, err := reader.Step()
		if err != nil {
			exitError(err)
		}
		if !ok {
			break
		}
		entry, err := reader.GetEntry()
		if err != nil {
			exitError(err)
		}
		writeTextField(buffered, []byte("__REALTIME_TIMESTAMP"), []byte(strconv.FormatUint(entry.Realtime, 10)))
		writeTextField(buffered, []byte("__MONOTONIC_TIMESTAMP"), []byte(strconv.FormatUint(entry.Monotonic, 10)))
		writeTextField(buffered, []byte("__SEQNUM"), []byte(strconv.FormatUint(entry.Seqnum, 10)))
		writeTextField(buffered, []byte("__BOOT_ID"), []byte(hex.EncodeToString(entry.BootID[:])))
		for _, payload := range entry.Payloads {
			name, value := splitPayload(payload)
			if name == nil {
				exitError(fmt.Errorf("payload without '=' in %s", sanitizedFileID(*input)))
			}
			writeExportField(buffered, name, value)
		}
		_, _ = buffered.Write([]byte{'\n'})
	}
}

func writeSpool(args []string) {
	fs := flag.NewFlagSet("write-spool", flag.ExitOnError)
	input := fs.String("input", "-", "spool input path or - for stdin")
	output := fs.String("output", "", "journal output path")
	format := fs.String("format", "regular", "journal format: regular or compact")
	compressionName := fs.String("compression", "none", "compression: none, zstd, xz, or lz4")
	fss := fs.Bool("fss", false, "enable deterministic synthetic Forward Secure Sealing")
	fssIntervalUsec := fs.Uint64("fss-interval-usec", 1_000_000, "FSS interval")
	finalState := fs.String("final-state", "offline", "final state: online or offline")
	maxSize := fs.Uint64("max-size-bytes", defaultMaxSize, "hash-table sizing max file size")
	livePublishEveryEntries := fs.Uint64("live-publish-every-entries", 1, "live publication cadence")
	_ = fs.Parse(args)
	if *output == "" {
		exitError(errors.New("--output is required"))
	}
	compact := false
	switch *format {
	case "regular":
		compact = false
	case "compact":
		compact = true
	default:
		exitError(fmt.Errorf("invalid --format: %s", *format))
	}
	compression, err := parseCompression(*compressionName)
	if err != nil {
		exitError(err)
	}
	in, closeInput, err := openInput(*input)
	if err != nil {
		exitError(err)
	}
	defer closeInput()
	parser := newSpoolParser(in)
	parseStarted := time.Now()
	first, ok, err := parser.next()
	parseSeconds := time.Since(parseStarted).Seconds()
	if err != nil {
		exitError(err)
	}
	if !ok {
		exitError(errors.New("spool contains no entries"))
	}
	if err := os.MkdirAll(filepath.Dir(*output), 0o755); err != nil {
		exitError(err)
	}
	_ = os.Remove(*output)
	bootID := first.BootID
	if isZeroUUID(bootID) {
		bootID = mustUUID("dddddddddddddddddddddddddddddddd")
	}
	writerOptions := journal.Options{
		MachineID:               mustUUID("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		BootID:                  bootID,
		SeqnumID:                mustUUID("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		FileID:                  randomUUID(),
		HeadSeqnum:              max(1, first.Seqnum),
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
			StartUsec:    systemdFSSStartUsec(first.Realtime, *fssIntervalUsec),
		}
	}
	createStarted := time.Now()
	w, err := journal.Create(*output, writerOptions)
	createSeconds := time.Since(createStarted).Seconds()
	if err != nil {
		exitError(err)
	}
	var records, payloads, payloadBytes uint64
	var appendSeconds float64
	appendOne := func(entry spoolEntry) {
		started := time.Now()
		err := w.AppendRaw(entry.Payloads, journal.EntryOptions{
			RealtimeUsec:     entry.Realtime,
			RealtimeUsecSet:  true,
			MonotonicUsec:    entry.Monotonic,
			MonotonicUsecSet: true,
			BootID:           entry.BootID,
			Seqnum:           entry.Seqnum,
		})
		appendSeconds += time.Since(started).Seconds()
		if err != nil {
			exitError(err)
		}
		records++
		payloads += uint64(len(entry.Payloads))
		for _, payload := range entry.Payloads {
			payloadBytes += uint64(len(payload))
		}
	}
	appendOne(first)
	for {
		started := time.Now()
		entry, ok, err := parser.next()
		parseSeconds += time.Since(started).Seconds()
		if err != nil {
			exitError(err)
		}
		if !ok {
			break
		}
		appendOne(entry)
	}
	closeStarted := time.Now()
	switch *finalState {
	case "offline":
		err = w.CloseOffline()
	case "online":
		err = w.Close()
	default:
		err = fmt.Errorf("invalid --final-state: %s", *finalState)
	}
	closeSeconds := time.Since(closeStarted).Seconds()
	if err != nil {
		exitError(err)
	}
	total := parseSeconds + createSeconds + appendSeconds + closeSeconds
	result := map[string]any{
		"schema":                       spoolSchema,
		"driver":                       "go",
		"status":                       "ok",
		"records":                      records,
		"payloads":                     payloads,
		"payload_bytes":                payloadBytes,
		"generated_bytes":              fileSize(*output),
		"format":                       *format,
		"compression":                  *compressionName,
		"fss":                          *fss,
		"final_state":                  *finalState,
		"parse_seconds":                parseSeconds,
		"create_seconds":               createSeconds,
		"append_seconds":               appendSeconds,
		"close_seconds":                closeSeconds,
		"total_seconds":                total,
		"append_entries_per_second":    rate(records, appendSeconds),
		"total_entries_per_second":     rate(records, total),
		"append_payloads_per_second":   rate(payloads, appendSeconds),
		"append_payload_bytes_per_sec": rate(payloadBytes, appendSeconds),
	}
	_ = json.NewEncoder(os.Stdout).Encode(result)
}

type spoolParser struct {
	reader *bufio.Reader
}

func newSpoolParser(reader io.Reader) *spoolParser {
	return &spoolParser{reader: bufio.NewReaderSize(reader, 1024*1024)}
}

func (p *spoolParser) next() (spoolEntry, bool, error) {
	var entry spoolEntry
	for {
		line, err := p.reader.ReadBytes('\n')
		if err != nil {
			if errors.Is(err, io.EOF) {
				if len(entry.Payloads) > 0 {
					return entry, true, nil
				}
				return spoolEntry{}, false, nil
			}
			return spoolEntry{}, false, err
		}
		if string(line) == "\n" {
			if len(entry.Payloads) > 0 {
				return entry, true, nil
			}
			continue
		}
		if len(line) == 0 || line[len(line)-1] != '\n' {
			return spoolEntry{}, false, errors.New("truncated spool field line")
		}
		line = line[:len(line)-1]
		var name, value []byte
		if idx := bytesIndex(line, '='); idx >= 0 {
			name = append([]byte(nil), line[:idx]...)
			value = append([]byte(nil), line[idx+1:]...)
		} else {
			name = append([]byte(nil), line...)
			sizeRaw := make([]byte, 8)
			if _, err := io.ReadFull(p.reader, sizeRaw); err != nil {
				return spoolEntry{}, false, err
			}
			size := binary.LittleEndian.Uint64(sizeRaw)
			if size > 768*1024*1024 {
				return spoolEntry{}, false, errors.New("spool field exceeds journal DATA size limit")
			}
			value = make([]byte, int(size))
			if _, err := io.ReadFull(p.reader, value); err != nil {
				return spoolEntry{}, false, err
			}
			trailer, err := p.reader.ReadByte()
			if err != nil {
				return spoolEntry{}, false, err
			}
			if trailer != '\n' {
				return spoolEntry{}, false, errors.New("spool binary field missing newline trailer")
			}
		}
		if string(name) == "__REALTIME_TIMESTAMP" {
			entry.Realtime = parseU64(value)
			continue
		}
		if string(name) == "__MONOTONIC_TIMESTAMP" {
			entry.Monotonic = parseU64(value)
			continue
		}
		if string(name) == "__SEQNUM" {
			entry.Seqnum = parseU64(value)
			continue
		}
		if string(name) == "__BOOT_ID" {
			entry.BootID = mustUUID(string(value))
			continue
		}
		payload := make([]byte, 0, len(name)+1+len(value))
		payload = append(payload, name...)
		payload = append(payload, '=')
		payload = append(payload, value...)
		entry.Payloads = append(entry.Payloads, payload)
	}
}

func inputPaths(input, directory string, limit int) ([]string, error) {
	var paths []string
	if input != "" {
		paths = append(paths, input)
	}
	if directory != "" {
		err := filepath.WalkDir(directory, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() {
				return nil
			}
			if journalLike(path) {
				paths = append(paths, path)
			}
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	sort.Strings(paths)
	if limit > 0 && len(paths) > limit {
		paths = paths[:limit]
	}
	if len(paths) == 0 {
		return nil, errors.New("no input files")
	}
	return paths, nil
}

func journalLike(path string) bool {
	return strings.HasSuffix(path, ".journal") || strings.HasSuffix(path, ".journal~") ||
		strings.HasSuffix(path, ".journal.zst") || strings.HasSuffix(path, ".journal~.zst")
}

func writeRawCSV(rows []map[string]any) {
	w := csv.NewWriter(os.Stdout)
	defer w.Flush()
	header := []string{"schema", "driver", "status", "file_id", "input_bytes", "entries", "payloads", "payload_bytes", "binary_payloads", "payloads_without_equals", "largest_payload_bytes", "hash", "elapsed_seconds", "entries_per_second", "payloads_per_second", "payload_bytes_per_second", "input_bytes_per_second", "reader_path", "error_class", "error_sha256"}
	_ = w.Write(header)
	for _, row := range rows {
		record := make([]string, len(header))
		for i, key := range header {
			record[i] = fmt.Sprint(row[key])
		}
		_ = w.Write(record)
	}
}

func errorRow(path, class, msg string) map[string]any {
	sum := sha256.Sum256([]byte(msg))
	return map[string]any{
		"schema":       rawReaderSchema,
		"driver":       "go",
		"status":       "failed",
		"file_id":      sanitizedFileID(path),
		"error_class":  class,
		"error_sha256": hex.EncodeToString(sum[:]),
	}
}

func writeTextField(w *bufio.Writer, name, value []byte) {
	_, _ = w.Write(name)
	_ = w.WriteByte('=')
	_, _ = w.Write(value)
	_ = w.WriteByte('\n')
}

func writeExportField(w *bufio.Writer, name, value []byte) {
	if payloadHasBinary(value) {
		_, _ = w.Write(name)
		_ = w.WriteByte('\n')
		var size [8]byte
		binary.LittleEndian.PutUint64(size[:], uint64(len(value)))
		_, _ = w.Write(size[:])
		_, _ = w.Write(value)
		_ = w.WriteByte('\n')
		return
	}
	writeTextField(w, name, value)
}

func splitPayload(payload []byte) ([]byte, []byte) {
	if idx := bytesIndex(payload, '='); idx >= 0 {
		return payload[:idx], payload[idx+1:]
	}
	return nil, nil
}

func payloadName(payload []byte) []byte {
	name, _ := splitPayload(payload)
	return name
}

func payloadHasBinary(payload []byte) bool {
	for _, b := range payload {
		if b < 32 && b != '\t' {
			return true
		}
	}
	return false
}

func bytesIndex(buf []byte, want byte) int {
	for i, b := range buf {
		if b == want {
			return i
		}
	}
	return -1
}

func openInput(path string) (io.Reader, func(), error) {
	if path == "-" {
		return os.Stdin, func() {}, nil
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, func() {}, err
	}
	return file, func() { _ = file.Close() }, nil
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

func mustUUID(s string) journal.UUID {
	raw, err := hex.DecodeString(strings.ReplaceAll(s, "-", ""))
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

func isZeroUUID(id journal.UUID) bool {
	for _, b := range id {
		if b != 0 {
			return false
		}
	}
	return true
}

func parseU64(value []byte) uint64 {
	n, err := strconv.ParseUint(string(value), 10, 64)
	if err != nil {
		return 0
	}
	return n
}

func sanitizedFileID(path string) string {
	abs, _ := filepath.Abs(path)
	stat, _ := os.Stat(path)
	seed := abs
	if stat != nil {
		seed = fmt.Sprintf("%s:%d:%d", abs, stat.Size(), stat.ModTime().UnixNano())
	}
	sum := sha256.Sum256([]byte(seed))
	return hex.EncodeToString(sum[:])[:24]
}

func fileSize(path string) int64 {
	stat, err := os.Stat(path)
	if err != nil {
		return 0
	}
	return stat.Size()
}

func rate(value uint64, seconds float64) float64 {
	if seconds <= 0 {
		return 0
	}
	return float64(value) / seconds
}

func exitError(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}
