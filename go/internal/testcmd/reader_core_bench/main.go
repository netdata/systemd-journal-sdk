package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"math/bits"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"strconv"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const defaultWindowSize = 32 * 1024 * 1024

type inputsFlag []string

func (f *inputsFlag) String() string {
	return strings.Join(*f, ",")
}

func (f *inputsFlag) Set(value string) error {
	*f = append(*f, value)
	return nil
}

type counts struct {
	records  uint64
	fields   uint64
	bytes    uint64
	checksum uint64
}

type benchConfig struct {
	inputs       []string
	mode         string
	surface      string
	direction    string
	bounds       string
	mmapStrategy string
	windowSize   uint64
	cpuProfile   string
	memProfile   string
	loops        int
	options      journal.ReaderOptions
}

func (c *counts) addRun(other counts) {
	c.records += other.records
	c.fields += other.fields
	c.bytes += other.bytes
	c.checksum = bits.RotateLeft64(c.checksum, 11) ^ other.checksum
}

func (c *counts) addPayload(payload []byte) {
	c.fields++
	c.bytes += uint64(len(payload))
	c.checksum = checksumPayload(c.checksum, payload)
}

func (c *counts) addRecordMarker(value uint64) {
	c.records++
	c.checksum = bits.RotateLeft64(c.checksum, 7) ^ value
}

func checksumPayload(checksum uint64, payload []byte) uint64 {
	checksum = bits.RotateLeft64(checksum, 5) ^ uint64(len(payload))
	if len(payload) > 0 {
		checksum ^= uint64(payload[0]) << 8
		checksum ^= uint64(payload[len(payload)-1])
	}
	return checksum
}

func parseOptions(bounds, mmapStrategy string) (journal.ReaderOptions, error) {
	opts := journal.DefaultReaderOptions()
	switch bounds {
	case "live":
		opts = opts.WithBounds(journal.ReaderBoundsLive)
	case "snapshot":
		opts = opts.WithBounds(journal.ReaderBoundsSnapshot)
	default:
		return opts, fmt.Errorf("invalid --bounds: %s", bounds)
	}
	switch mmapStrategy {
	case "read-at", "buffer":
		opts = opts.WithAccessMode(journal.ReaderAccessReadAt)
	case "mmap", "whole-file":
		opts = opts.WithAccessMode(journal.ReaderAccessMmap)
	default:
		return opts, fmt.Errorf("invalid --mmap-strategy: %s", mmapStrategy)
	}
	return opts, nil
}

func processStatusKB() map[string]uint64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return map[string]uint64{}
	}
	wanted := map[string]struct{}{
		"VmSize": {}, "VmPeak": {}, "VmRSS": {}, "VmHWM": {}, "RssAnon": {},
		"RssFile": {}, "RssShmem": {}, "VmData": {}, "VmStk": {}, "VmExe": {},
		"VmLib": {}, "VmPTE": {},
	}
	out := make(map[string]uint64)
	for _, line := range strings.Split(string(data), "\n") {
		key, rest, ok := strings.Cut(line, ":")
		if !ok {
			continue
		}
		if _, ok := wanted[key]; !ok {
			continue
		}
		fields := strings.Fields(rest)
		if len(fields) == 0 {
			continue
		}
		value, err := strconv.ParseUint(fields[0], 10, 64)
		if err == nil {
			out[key+"_kb"] = value
		}
	}
	return out
}

type sdkReader interface {
	Close() error
	SeekHead() error
	SeekTail() error
	Step() (bool, error)
	StepBack() (bool, error)
	GetEntry() (*journal.Entry, error)
	GetRealtimeUsec() (uint64, error)
	VisitEntryPayloads(func([]byte) error) error
}

func openSDKReader(surface string, inputs []string, opts journal.ReaderOptions) (sdkReader, error) {
	switch surface {
	case "file":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("file surface requires exactly one --input")
		}
		return journal.OpenFileWithOptions(inputs[0], opts)
	case "open-files":
		return journal.OpenFilesWithOptions(inputs, opts)
	case "directory":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("directory surface requires exactly one --input")
		}
		return journal.OpenDirectoryWithOptions(inputs[0], opts)
	default:
		return nil, fmt.Errorf("invalid --surface: %s", surface)
	}
}

func seekSDKReader(reader sdkReader, direction string) error {
	if direction == "backward" {
		return reader.SeekTail()
	}
	return reader.SeekHead()
}

func stepSDKReader(reader sdkReader, direction string) (bool, error) {
	if direction == "backward" {
		return reader.StepBack()
	}
	return reader.Step()
}

func addSDKEntry(result *counts, reader sdkReader) error {
	entry, err := reader.GetEntry()
	if err != nil {
		return err
	}
	result.addRecordMarker(entry.Realtime)
	for _, payload := range entry.Payloads {
		result.addPayload(payload)
	}
	return nil
}

func addSDKPayloads(result *counts, reader sdkReader) error {
	realtime, err := reader.GetRealtimeUsec()
	if err != nil {
		return err
	}
	result.addRecordMarker(realtime)
	return reader.VisitEntryPayloads(func(payload []byte) error {
		result.addPayload(payload)
		return nil
	})
}

func readSDK(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	reader, err := openSDKReader(surface, inputs, opts)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	if err := seekSDKReader(reader, direction); err != nil {
		return counts{}, err
	}

	var result counts
	for {
		ok, err := stepSDKReader(reader, direction)
		if err != nil {
			return counts{}, err
		}
		if !ok {
			break
		}
		switch mode {
		case "sdk-entry":
			err = addSDKEntry(&result, reader)
		case "sdk-payloads":
			err = addSDKPayloads(&result, reader)
		default:
			return counts{}, fmt.Errorf("invalid SDK mode: %s", mode)
		}
		if err != nil {
			return counts{}, err
		}
	}
	return result, nil
}

type facadeHandle interface {
	Close() error
	SeekHead() error
	SeekTail() error
	Next() (int, error)
	Previous() (int, error)
	GetRealtimeUsec() (uint64, error)
	RestartData() error
	EnumerateAvailableData() ([]byte, bool, error)
}

func openFacade(surface string, inputs []string, opts journal.ReaderOptions) (facadeHandle, error) {
	var (
		j   facadeHandle
		err error
	)
	switch surface {
	case "file", "open-files":
		j, err = journal.SdJournalOpenFilesWithOptions(inputs, 0, opts)
	case "directory":
		if len(inputs) != 1 {
			return nil, fmt.Errorf("directory surface requires exactly one --input")
		}
		j, err = journal.SdJournalOpenDirectoryWithOptions(inputs[0], 0, opts)
	default:
		return nil, fmt.Errorf("invalid --surface: %s", surface)
	}
	return j, err
}

func seekFacade(j facadeHandle, direction string) error {
	if direction == "backward" {
		return j.SeekTail()
	}
	return j.SeekHead()
}

func stepFacade(j facadeHandle, direction string) (int, error) {
	if direction == "backward" {
		return j.Previous()
	}
	return j.Next()
}

func addFacadeNext(result *counts, j facadeHandle) error {
	realtime, err := j.GetRealtimeUsec()
	if err != nil {
		return err
	}
	result.addRecordMarker(realtime)
	return nil
}

func addFacadeData(result *counts, j facadeHandle) error {
	if err := addFacadeNext(result, j); err != nil {
		return err
	}
	if err := j.RestartData(); err != nil {
		return err
	}
	for {
		payload, ok, err := j.EnumerateAvailableData()
		if err != nil || !ok {
			return err
		}
		result.addPayload(payload)
	}
}

func readFacade(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	j, err := openFacade(surface, inputs, opts)
	if err != nil {
		return counts{}, err
	}
	defer j.Close()
	if err := seekFacade(j, direction); err != nil {
		return counts{}, err
	}

	var result counts
	for {
		n, err := stepFacade(j, direction)
		if err != nil {
			return counts{}, err
		}
		if n == 0 {
			break
		}
		switch mode {
		case "facade-next":
			err = addFacadeNext(&result, j)
		case "facade-data":
			err = addFacadeData(&result, j)
		default:
			return counts{}, fmt.Errorf("invalid facade mode: %s", mode)
		}
		if err != nil {
			return counts{}, err
		}
	}
	return result, nil
}

func parseBenchConfig() (benchConfig, error) {
	var inputs inputsFlag
	mode := flag.String("mode", "sdk-payloads", "")
	surface := flag.String("surface", "file", "")
	direction := flag.String("direction", "forward", "")
	bounds := flag.String("bounds", "live", "")
	mmapStrategy := flag.String("mmap-strategy", "read-at", "")
	windowSize := flag.Uint64("window-size", defaultWindowSize, "")
	cpuProfile := flag.String("cpuprofile", "", "")
	memProfile := flag.String("memprofile", "", "")
	loops := flag.Int("loops", 1, "")
	flag.Var(&inputs, "input", "")
	flag.Parse()
	if len(inputs) == 0 {
		return benchConfig{}, errors.New("missing --input")
	}
	if *direction != "forward" && *direction != "backward" {
		return benchConfig{}, fmt.Errorf("invalid --direction: %s", *direction)
	}
	if *loops < 1 {
		return benchConfig{}, fmt.Errorf("invalid --loops: %d", *loops)
	}
	opts, err := parseOptions(*bounds, *mmapStrategy)
	if err != nil {
		return benchConfig{}, err
	}
	return benchConfig{
		inputs:       inputs,
		mode:         *mode,
		surface:      *surface,
		direction:    *direction,
		bounds:       *bounds,
		mmapStrategy: *mmapStrategy,
		windowSize:   *windowSize,
		cpuProfile:   *cpuProfile,
		memProfile:   *memProfile,
		loops:        *loops,
		options:      opts,
	}, nil
}

func main() {
	cfg, err := parseBenchConfig()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	before := processStatusKB()
	if cfg.cpuProfile != "" {
		f, err := os.Create(cfg.cpuProfile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "create cpu profile: %v\n", err)
			os.Exit(1)
		}
		if err := pprof.StartCPUProfile(f); err != nil {
			_ = f.Close()
			fmt.Fprintf(os.Stderr, "start cpu profile: %v\n", err)
			os.Exit(1)
		}
		defer f.Close()
		defer pprof.StopCPUProfile()
	}
	started := time.Now()
	result, err := runBenchLoops(cfg)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	seconds := time.Since(started).Seconds()
	if err := writeMemProfile(cfg.memProfile); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	after := processStatusKB()
	encoded, err := benchOutput(cfg, result, seconds, before, after)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println(string(encoded))
}

func runBenchLoops(cfg benchConfig) (counts, error) {
	var result counts
	for range cfg.loops {
		partial, err := readBenchOnce(cfg)
		if err != nil {
			return counts{}, err
		}
		result.addRun(partial)
	}
	return result, nil
}

func readBenchOnce(cfg benchConfig) (counts, error) {
	switch cfg.mode {
	case "sdk-entry", "sdk-payloads":
		return readSDK(cfg.surface, cfg.mode, cfg.direction, cfg.inputs, cfg.options)
	case "facade-next", "facade-data":
		return readFacade(cfg.surface, cfg.mode, cfg.direction, cfg.inputs, cfg.options)
	default:
		return counts{}, fmt.Errorf("invalid --mode: %s", cfg.mode)
	}
}

func writeMemProfile(path string) error {
	if path == "" {
		return nil
	}
	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create memory profile: %w", err)
	}
	runtime.GC()
	if err := pprof.WriteHeapProfile(f); err != nil {
		_ = f.Close()
		return fmt.Errorf("write memory profile: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("close memory profile: %w", err)
	}
	return nil
}

func benchOutput(
	cfg benchConfig,
	result counts,
	seconds float64,
	before map[string]uint64,
	after map[string]uint64,
) ([]byte, error) {
	output := map[string]interface{}{
		"language":               "go",
		"surface":                cfg.surface,
		"mode":                   cfg.mode,
		"direction":              cfg.direction,
		"records":                result.records,
		"fields":                 result.fields,
		"bytes":                  result.bytes,
		"checksum":               result.checksum,
		"read_seconds":           seconds,
		"read_rows_per_second":   float64(result.records) / seconds,
		"read_fields_per_second": float64(result.fields) / seconds,
		"read_bytes_per_second":  float64(result.bytes) / seconds,
		"inputs":                 absoluteInputs(cfg.inputs),
		"window_size":            cfg.windowSize,
		"bounds":                 cfg.bounds,
		"mmap_strategy":          cfg.mmapStrategy,
		"loops":                  cfg.loops,
		"timer_excludes":         []string{"fixture generation", "process startup", "external verification"},
		"process_status_before":  before,
		"process_status_after":   after,
		"errors":                 []string{},
	}
	return json.Marshal(output)
}

func absoluteInputs(inputs []string) []string {
	absInputs := make([]string, 0, len(inputs))
	for _, input := range inputs {
		abs, err := filepath.Abs(input)
		if err != nil {
			absInputs = append(absInputs, input)
		} else {
			absInputs = append(absInputs, abs)
		}
	}
	return absInputs
}
