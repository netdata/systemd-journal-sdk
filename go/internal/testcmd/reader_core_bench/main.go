package main

import (
	"encoding/json"
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

func openSDKReader(surface string, inputs []string, opts journal.ReaderOptions) (interface {
	Close() error
	SeekHead() error
	SeekTail() error
	Step() (bool, error)
	StepBack() (bool, error)
	GetEntry() (*journal.Entry, error)
	GetRealtimeUsec() (uint64, error)
	VisitEntryPayloads(func([]byte) error) error
}, error) {
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

func readSDK(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	reader, err := openSDKReader(surface, inputs, opts)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	if direction == "backward" {
		if err := reader.SeekTail(); err != nil {
			return counts{}, err
		}
	} else {
		if err := reader.SeekHead(); err != nil {
			return counts{}, err
		}
	}

	var result counts
	for {
		var ok bool
		if direction == "backward" {
			ok, err = reader.StepBack()
		} else {
			ok, err = reader.Step()
		}
		if err != nil {
			return counts{}, err
		}
		if !ok {
			break
		}
		switch mode {
		case "sdk-entry":
			entry, err := reader.GetEntry()
			if err != nil {
				return counts{}, err
			}
			result.addRecordMarker(entry.Realtime)
			for _, payload := range entry.Payloads {
				result.addPayload(payload)
			}
		case "sdk-payloads":
			realtime, err := reader.GetRealtimeUsec()
			if err != nil {
				return counts{}, err
			}
			result.addRecordMarker(realtime)
			if err := reader.VisitEntryPayloads(func(payload []byte) error {
				result.addPayload(payload)
				return nil
			}); err != nil {
				return counts{}, err
			}
		default:
			return counts{}, fmt.Errorf("invalid SDK mode: %s", mode)
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

func readFacade(surface, mode, direction string, inputs []string, opts journal.ReaderOptions) (counts, error) {
	var (
		j   facadeHandle
		err error
	)
	switch surface {
	case "file", "open-files":
		j, err = journal.SdJournalOpenFilesWithOptions(inputs, 0, opts)
	case "directory":
		if len(inputs) != 1 {
			return counts{}, fmt.Errorf("directory surface requires exactly one --input")
		}
		j, err = journal.SdJournalOpenDirectoryWithOptions(inputs[0], 0, opts)
	default:
		return counts{}, fmt.Errorf("invalid --surface: %s", surface)
	}
	if err != nil {
		return counts{}, err
	}
	defer j.Close()
	if direction == "backward" {
		if err := j.SeekTail(); err != nil {
			return counts{}, err
		}
	} else {
		if err := j.SeekHead(); err != nil {
			return counts{}, err
		}
	}

	var result counts
	for {
		var n int
		if direction == "backward" {
			n, err = j.Previous()
		} else {
			n, err = j.Next()
		}
		if err != nil {
			return counts{}, err
		}
		if n == 0 {
			break
		}
		switch mode {
		case "facade-next":
			realtime, err := j.GetRealtimeUsec()
			if err != nil {
				return counts{}, err
			}
			result.addRecordMarker(realtime)
		case "facade-data":
			realtime, err := j.GetRealtimeUsec()
			if err != nil {
				return counts{}, err
			}
			result.addRecordMarker(realtime)
			if err := j.RestartData(); err != nil {
				return counts{}, err
			}
			for {
				payload, ok, err := j.EnumerateAvailableData()
				if err != nil {
					return counts{}, err
				}
				if !ok {
					break
				}
				result.addPayload(payload)
			}
		default:
			return counts{}, fmt.Errorf("invalid facade mode: %s", mode)
		}
	}
	return result, nil
}

func main() {
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
		fmt.Fprintln(os.Stderr, "missing --input")
		os.Exit(2)
	}
	if *direction != "forward" && *direction != "backward" {
		fmt.Fprintf(os.Stderr, "invalid --direction: %s\n", *direction)
		os.Exit(2)
	}
	if *loops < 1 {
		fmt.Fprintf(os.Stderr, "invalid --loops: %d\n", *loops)
		os.Exit(2)
	}
	opts, err := parseOptions(*bounds, *mmapStrategy)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	_ = windowSize // Go currently benchmarks read-at and whole-file mmap only.

	before := processStatusKB()
	if *cpuProfile != "" {
		f, err := os.Create(*cpuProfile)
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
	var result counts
	for i := 0; i < *loops; i++ {
		var partial counts
		switch *mode {
		case "sdk-entry", "sdk-payloads":
			partial, err = readSDK(*surface, *mode, *direction, inputs, opts)
		case "facade-next", "facade-data":
			partial, err = readFacade(*surface, *mode, *direction, inputs, opts)
		default:
			err = fmt.Errorf("invalid --mode: %s", *mode)
		}
		if err != nil {
			break
		}
		result.addRun(partial)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	seconds := time.Since(started).Seconds()
	if *memProfile != "" {
		f, err := os.Create(*memProfile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "create memory profile: %v\n", err)
			os.Exit(1)
		}
		runtime.GC()
		if err := pprof.WriteHeapProfile(f); err != nil {
			_ = f.Close()
			fmt.Fprintf(os.Stderr, "write memory profile: %v\n", err)
			os.Exit(1)
		}
		if err := f.Close(); err != nil {
			fmt.Fprintf(os.Stderr, "close memory profile: %v\n", err)
			os.Exit(1)
		}
	}
	after := processStatusKB()
	absInputs := make([]string, 0, len(inputs))
	for _, input := range inputs {
		abs, err := filepath.Abs(input)
		if err != nil {
			absInputs = append(absInputs, input)
		} else {
			absInputs = append(absInputs, abs)
		}
	}
	output := map[string]interface{}{
		"language":               "go",
		"surface":                *surface,
		"mode":                   *mode,
		"direction":              *direction,
		"records":                result.records,
		"fields":                 result.fields,
		"bytes":                  result.bytes,
		"checksum":               result.checksum,
		"read_seconds":           seconds,
		"read_rows_per_second":   float64(result.records) / seconds,
		"read_fields_per_second": float64(result.fields) / seconds,
		"read_bytes_per_second":  float64(result.bytes) / seconds,
		"inputs":                 absInputs,
		"window_size":            *windowSize,
		"bounds":                 *bounds,
		"mmap_strategy":          *mmapStrategy,
		"loops":                  *loops,
		"timer_excludes":         []string{"fixture generation", "process startup", "external verification"},
		"process_status_before":  before,
		"process_status_after":   after,
		"errors":                 []string{},
	}
	encoded, err := json.Marshal(output)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println(string(encoded))
}
