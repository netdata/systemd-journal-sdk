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
	"hash"
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

type rawReadConfig struct {
	path           string
	access         string
	hashMode       string
	binaryStats    bool
	separatorStats bool
}

type rawReadState struct {
	counts rawCounts
	hasher hash.Hash
	lenBuf [8]byte
}

type writeSpoolConfig struct {
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

type spoolWriteStats struct {
	records       uint64
	payloads      uint64
	payloadBytes  uint64
	parseSeconds  float64
	createSeconds float64
	appendSeconds float64
	closeSeconds  float64
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
	hashMode := fs.String("hash", "sha256", "payload hash mode: sha256 or none")
	binaryStats := fs.Bool("binary-stats", true, "count payloads containing binary bytes")
	separatorStats := fs.Bool("separator-stats", true, "count payloads missing the FIELD=value separator")
	_ = fs.Parse(args)

	paths, err := inputPaths(*input, *directory, *limit)
	if err != nil {
		exitError(err)
	}
	rows := make([]map[string]any, 0, len(paths))
	for _, path := range paths {
		row := rawReadOne(path, *access, *hashMode, *binaryStats, *separatorStats)
		rows = append(rows, row)
	}
	if *output == "json" {
		_ = json.NewEncoder(os.Stdout).Encode(rows)
		return
	}
	writeRawCSV(rows)
}

func rawReadOne(path, access, hashMode string, binaryStats bool, separatorStats bool) map[string]any {
	cfg := rawReadConfig{path: path, access: access, hashMode: hashMode, binaryStats: binaryStats, separatorStats: separatorStats}
	started := time.Now()
	reader, errClass, err := openRawReader(cfg)
	if err != nil {
		return errorRow(path, errClass, err.Error())
	}
	defer reader.Close()
	state, err := newRawReadState(hashMode)
	if err != nil {
		return errorRow(path, "invalid_hash", err.Error())
	}
	if errClass, err := scanRawReader(reader, cfg, &state); err != nil {
		return errorRow(path, errClass, err.Error())
	}
	elapsed := time.Since(started).Seconds()
	return rawReadResult(cfg, state, elapsed)
}

func openRawReader(cfg rawReadConfig) (*journal.Reader, string, error) {
	opts := journal.DefaultReaderOptions().WithBounds(journal.ReaderBoundsSnapshot)
	switch cfg.access {
	case "mmap":
		opts = opts.WithAccessMode(journal.ReaderAccessMmap)
	case "read-at":
		opts = opts.WithAccessMode(journal.ReaderAccessReadAt)
	default:
		return nil, "invalid_access", fmt.Errorf("invalid access %q", cfg.access)
	}
	reader, err := journal.OpenFileWithOptions(cfg.path, opts)
	if err != nil {
		return nil, "open", err
	}
	if err := reader.SeekHead(); err != nil {
		_ = reader.Close()
		return nil, "seek", err
	}
	return reader, "", nil
}

func newRawReadState(hashMode string) (rawReadState, error) {
	var state rawReadState
	switch hashMode {
	case "none":
	case "sha256":
		state.hasher = sha256.New()
		_, _ = state.hasher.Write([]byte(rawReaderMagic))
	default:
		return state, fmt.Errorf("invalid hash mode %q", hashMode)
	}
	return state, nil
}

func scanRawReader(reader *journal.Reader, cfg rawReadConfig, state *rawReadState) (string, error) {
	for {
		ok, err := reader.Step()
		if err != nil {
			return "step", err
		}
		if !ok {
			break
		}
		markRawEntryStart(state)
		err = reader.VisitEntryPayloads(func(payload []byte) error {
			addRawPayload(state, cfg, payload)
			return nil
		})
		if err != nil {
			return "payload", err
		}
		markRawEntryEnd(state)
	}
	return "", nil
}

func markRawEntryStart(state *rawReadState) {
	if state.hasher == nil {
		return
	}
	_, _ = state.hasher.Write([]byte("E"))
	binary.BigEndian.PutUint64(state.lenBuf[:], state.counts.Entries)
	_, _ = state.hasher.Write(state.lenBuf[:])
}

func markRawEntryEnd(state *rawReadState) {
	if state.hasher != nil {
		_, _ = state.hasher.Write([]byte("e"))
	}
	state.counts.Entries++
}

func addRawPayload(state *rawReadState, cfg rawReadConfig, payload []byte) {
	if state.hasher != nil {
		_, _ = state.hasher.Write([]byte("P"))
		binary.BigEndian.PutUint64(state.lenBuf[:], uint64(len(payload)))
		_, _ = state.hasher.Write(state.lenBuf[:])
		_, _ = state.hasher.Write(payload)
	}
	state.counts.Payloads++
	state.counts.PayloadBytes += uint64(len(payload))
	if uint64(len(payload)) > state.counts.LargestPayloadBytes {
		state.counts.LargestPayloadBytes = uint64(len(payload))
	}
	if cfg.separatorStats && payloadName(payload) == nil {
		state.counts.PayloadsWithoutSeparator++
	}
	if cfg.binaryStats && payloadHasBinary(payload) {
		state.counts.BinaryPayloads++
	}
}

func rawReadResult(cfg rawReadConfig, state rawReadState, elapsed float64) map[string]any {
	counts := state.counts
	inputBytes := fileSize(cfg.path)
	var digest any
	if state.hasher != nil {
		digest = hex.EncodeToString(state.hasher.Sum(nil))
	}
	var binaryPayloads any
	if cfg.binaryStats {
		binaryPayloads = counts.BinaryPayloads
	}
	var payloadsWithoutEquals any
	if cfg.separatorStats {
		payloadsWithoutEquals = counts.PayloadsWithoutSeparator
	}
	return map[string]any{
		"schema":                   rawReaderSchema,
		"driver":                   "go",
		"status":                   "ok",
		"hash_mode":                cfg.hashMode,
		"binary_stats":             cfg.binaryStats,
		"separator_stats":          cfg.separatorStats,
		"file_id":                  sanitizedFileID(cfg.path),
		"input_bytes":              inputBytes,
		"entries":                  counts.Entries,
		"payloads":                 counts.Payloads,
		"payload_bytes":            counts.PayloadBytes,
		"binary_payloads":          binaryPayloads,
		"payloads_without_equals":  payloadsWithoutEquals,
		"largest_payload_bytes":    counts.LargestPayloadBytes,
		"hash":                     digest,
		"elapsed_seconds":          elapsed,
		"entries_per_second":       rate(counts.Entries, elapsed),
		"payloads_per_second":      rate(counts.Payloads, elapsed),
		"payload_bytes_per_second": rate(counts.PayloadBytes, elapsed),
		"input_bytes_per_second":   rate(uint64(inputBytes), elapsed),
		"reader_path":              rawReaderPath(cfg.hashMode, cfg.binaryStats, cfg.separatorStats),
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
	cfg, err := parseWriteSpoolConfig(args)
	if err != nil {
		exitError(err)
	}
	in, closeInput, err := openInput(cfg.input)
	if err != nil {
		exitError(err)
	}
	defer closeInput()
	parser := newSpoolParser(in)
	parseStarted := time.Now()
	first, ok, err := parser.next()
	stats := spoolWriteStats{parseSeconds: time.Since(parseStarted).Seconds()}
	if err != nil {
		exitError(err)
	}
	if !ok {
		exitError(errors.New("spool contains no entries"))
	}
	if err := os.MkdirAll(filepath.Dir(cfg.output), 0o755); err != nil {
		exitError(err)
	}
	_ = os.Remove(cfg.output)
	createStarted := time.Now()
	w, err := journal.Create(cfg.output, writeSpoolOptions(cfg, first))
	stats.createSeconds = time.Since(createStarted).Seconds()
	if err != nil {
		exitError(err)
	}
	appendSpoolEntries(w, parser, first, &stats)
	closeStarted := time.Now()
	switch cfg.finalState {
	case "offline":
		err = w.CloseOffline()
	case "online":
		err = w.Close()
	default:
		err = fmt.Errorf("invalid --final-state: %s", cfg.finalState)
	}
	stats.closeSeconds = time.Since(closeStarted).Seconds()
	if err != nil {
		exitError(err)
	}
	_ = json.NewEncoder(os.Stdout).Encode(writeSpoolResult(cfg, stats))
}

func parseWriteSpoolConfig(args []string) (writeSpoolConfig, error) {
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
		return writeSpoolConfig{}, errors.New("--output is required")
	}
	compact, err := parseFormat(*format)
	if err != nil {
		return writeSpoolConfig{}, err
	}
	compression, err := parseCompression(*compressionName)
	if err != nil {
		return writeSpoolConfig{}, err
	}
	return writeSpoolConfig{
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

func parseFormat(format string) (bool, error) {
	switch format {
	case "regular":
		return false, nil
	case "compact":
		return true, nil
	default:
		return false, fmt.Errorf("invalid --format: %s", format)
	}
}

func writeSpoolOptions(cfg writeSpoolConfig, first spoolEntry) journal.Options {
	bootID := first.BootID
	if isZeroUUID(bootID) {
		bootID = mustUUID("dddddddddddddddddddddddddddddddd")
	}
	opts := journal.Options{
		MachineID:               mustUUID("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		BootID:                  bootID,
		SeqnumID:                mustUUID("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
		FileID:                  randomUUID(),
		HeadSeqnum:              max(1, first.Seqnum),
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
		opts.Seal = &journal.SealOptions{
			Seed:         make([]byte, 12),
			IntervalUsec: cfg.fssIntervalUsec,
			StartUsec:    systemdFSSStartUsec(first.Realtime, cfg.fssIntervalUsec),
		}
	}
	return opts
}

func appendSpoolEntries(w *journal.Writer, parser *spoolParser, first spoolEntry, stats *spoolWriteStats) {
	appendSpoolEntry(w, first, stats)
	for {
		started := time.Now()
		entry, ok, err := parser.next()
		stats.parseSeconds += time.Since(started).Seconds()
		if err != nil {
			exitError(err)
		}
		if !ok {
			break
		}
		appendSpoolEntry(w, entry, stats)
	}
}

func appendSpoolEntry(w *journal.Writer, entry spoolEntry, stats *spoolWriteStats) {
	started := time.Now()
	err := w.AppendRaw(entry.Payloads, journal.EntryOptions{
		RealtimeUsec:     entry.Realtime,
		RealtimeUsecSet:  true,
		MonotonicUsec:    entry.Monotonic,
		MonotonicUsecSet: true,
		BootID:           entry.BootID,
		Seqnum:           entry.Seqnum,
	})
	stats.appendSeconds += time.Since(started).Seconds()
	if err != nil {
		exitError(err)
	}
	stats.records++
	stats.payloads += uint64(len(entry.Payloads))
	for _, payload := range entry.Payloads {
		stats.payloadBytes += uint64(len(payload))
	}
}

func writeSpoolResult(cfg writeSpoolConfig, stats spoolWriteStats) map[string]any {
	total := stats.parseSeconds + stats.createSeconds + stats.appendSeconds + stats.closeSeconds
	result := map[string]any{
		"schema":                       spoolSchema,
		"driver":                       "go",
		"status":                       "ok",
		"records":                      stats.records,
		"payloads":                     stats.payloads,
		"payload_bytes":                stats.payloadBytes,
		"generated_bytes":              fileSize(cfg.output),
		"format":                       cfg.format,
		"compression":                  cfg.compressionName,
		"fss":                          cfg.fss,
		"final_state":                  cfg.finalState,
		"parse_seconds":                stats.parseSeconds,
		"create_seconds":               stats.createSeconds,
		"append_seconds":               stats.appendSeconds,
		"close_seconds":                stats.closeSeconds,
		"total_seconds":                total,
		"append_entries_per_second":    rate(stats.records, stats.appendSeconds),
		"total_entries_per_second":     rate(stats.records, total),
		"append_payloads_per_second":   rate(stats.payloads, stats.appendSeconds),
		"append_payload_bytes_per_sec": rate(stats.payloadBytes, stats.appendSeconds),
	}
	return result
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
			return finishSpoolRead(entry, err)
		}
		if isEntrySeparator(line) {
			if len(entry.Payloads) > 0 {
				return entry, true, nil
			}
			continue
		}
		if len(line) == 0 || line[len(line)-1] != '\n' {
			return spoolEntry{}, false, errors.New("truncated spool field line")
		}
		field, err := p.readSpoolField(line[:len(line)-1])
		if err != nil {
			return spoolEntry{}, false, err
		}
		if applySpoolMetadata(&entry, field.name, field.value) {
			continue
		}
		entry.Payloads = append(entry.Payloads, makePayload(field.name, field.value))
	}
}

type spoolField struct {
	name  []byte
	value []byte
}

func finishSpoolRead(entry spoolEntry, err error) (spoolEntry, bool, error) {
	if errors.Is(err, io.EOF) && len(entry.Payloads) > 0 {
		return entry, true, nil
	}
	if errors.Is(err, io.EOF) {
		return spoolEntry{}, false, nil
	}
	return spoolEntry{}, false, err
}

func isEntrySeparator(line []byte) bool {
	return string(line) == "\n"
}

func (p *spoolParser) readSpoolField(line []byte) (spoolField, error) {
	if idx := bytesIndex(line, '='); idx >= 0 {
		return spoolField{
			name:  append([]byte(nil), line[:idx]...),
			value: append([]byte(nil), line[idx+1:]...),
		}, nil
	}
	value, err := p.readBinarySpoolValue()
	if err != nil {
		return spoolField{}, err
	}
	return spoolField{name: append([]byte(nil), line...), value: value}, nil
}

func (p *spoolParser) readBinarySpoolValue() ([]byte, error) {
	sizeRaw := make([]byte, 8)
	if _, err := io.ReadFull(p.reader, sizeRaw); err != nil {
		return nil, err
	}
	size := binary.LittleEndian.Uint64(sizeRaw)
	if size > 768*1024*1024 {
		return nil, errors.New("spool field exceeds journal DATA size limit")
	}
	value := make([]byte, int(size))
	if _, err := io.ReadFull(p.reader, value); err != nil {
		return nil, err
	}
	trailer, err := p.reader.ReadByte()
	if err != nil {
		return nil, err
	}
	if trailer != '\n' {
		return nil, errors.New("spool binary field missing newline trailer")
	}
	return value, nil
}

func applySpoolMetadata(entry *spoolEntry, name, value []byte) bool {
	switch string(name) {
	case "__REALTIME_TIMESTAMP":
		entry.Realtime = parseU64(value)
	case "__MONOTONIC_TIMESTAMP":
		entry.Monotonic = parseU64(value)
	case "__SEQNUM":
		entry.Seqnum = parseU64(value)
	case "__BOOT_ID":
		entry.BootID = mustUUID(string(value))
	default:
		return false
	}
	return true
}

func makePayload(name, value []byte) []byte {
	payload := make([]byte, 0, len(name)+1+len(value))
	payload = append(payload, name...)
	payload = append(payload, '=')
	payload = append(payload, value...)
	return payload
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
	header := []string{"schema", "driver", "status", "file_id", "input_bytes", "entries", "payloads", "payload_bytes", "binary_payloads", "payloads_without_equals", "largest_payload_bytes", "hash", "hash_mode", "binary_stats", "separator_stats", "elapsed_seconds", "entries_per_second", "payloads_per_second", "payload_bytes_per_second", "input_bytes_per_second", "reader_path", "error_class", "error_sha256"}
	_ = w.Write(header)
	for _, row := range rows {
		record := make([]string, len(header))
		for i, key := range header {
			if row[key] != nil {
				record[i] = fmt.Sprint(row[key])
			}
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

func rawReaderPath(hashMode string, binaryStats bool, separatorStats bool) string {
	switch {
	case hashMode == "none" && !binaryStats && !separatorStats:
		return "raw-payload-visitor-no-content-scan"
	case hashMode == "none" && !binaryStats && separatorStats:
		return "raw-payload-visitor-key-delimiter-scan"
	case hashMode == "none" && binaryStats:
		return "raw-payload-visitor-binary-scan"
	case hashMode == "sha256" && !binaryStats:
		return "raw-payload-visitor-hash-full-payload"
	default:
		return "raw-payload-visitor-hash-and-binary-scan"
	}
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
