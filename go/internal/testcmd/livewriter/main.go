package main

import (
	"bytes"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type liveConfig struct {
	path              string
	readyFile         string
	entries           int
	delay             time.Duration
	syncEvery         int
	crashAfter        int
	binaryFixture     bool
	zstdFixture       bool
	xzFixture         bool
	lz4Fixture        bool
	compact           bool
	compression       int
	compressThreshold int
	seal              bool
	sealIntervalUsec  uint64
	sealStartUsec     uint64
}

func main() {
	cfg := parseLiveConfig()
	lock, err := journal.AcquireWriterLock(cfg.path)
	if err != nil {
		exitWithError("acquire writer lock", err)
	}
	defer releaseLock(lock)

	w, err := journal.Create(cfg.path, liveOptions(cfg))
	if err != nil {
		exitWithError("create journal", err)
	}
	appendLiveEntries(w, cfg)
}

func parseLiveConfig() liveConfig {
	var path string
	var readyFile string
	var entries int
	var delayText string
	var syncEvery int
	var crashAfter int
	var binaryFixture bool
	var compressionStr string
	var compressThreshold int
	var zstdFixture bool
	var xzFixture bool
	var lz4Fixture bool
	var compact bool
	var seal bool
	var sealIntervalUsec uint64
	var sealStartUsec uint64

	flag.StringVar(&path, "path", "", "journal path to create")
	flag.StringVar(&readyFile, "ready-file", "", "path to create after the first entry is committed")
	flag.IntVar(&entries, "entries", 1000, "number of entries to append")
	flag.StringVar(&delayText, "delay", "1ms", "delay between appends")
	flag.IntVar(&syncEvery, "sync-every", 25, "sync every N entries")
	flag.IntVar(&crashAfter, "crash-after", 0, "exit with status 17 after N entries without closing")
	flag.BoolVar(&binaryFixture, "binary-fixture", false, "write binary fields in the first entry")
	flag.StringVar(&compressionStr, "compression", "none", "compression: none, xz, lz4, zstd")
	flag.IntVar(&compressThreshold, "compress-threshold", 512, "minimum payload size for compression")
	flag.BoolVar(&zstdFixture, "zstd-fixture", false, "write zstd-compressed fields in the first entry")
	flag.BoolVar(&xzFixture, "xz-fixture", false, "write xz-compressed fields in the first entry")
	flag.BoolVar(&lz4Fixture, "lz4-fixture", false, "write lz4-compressed fields in the first entry")
	flag.BoolVar(&compact, "compact", false, "write the systemd compact journal format")
	flag.BoolVar(&seal, "seal", false, "enable Forward Secure Sealing with a deterministic zero seed")
	flag.Uint64Var(&sealIntervalUsec, "seal-interval-usec", 1_000_000, "FSS interval in microseconds")
	flag.Uint64Var(&sealStartUsec, "seal-start-usec", 1_700_001_000_000_000, "FSS start time in microseconds")
	flag.Parse()

	if path == "" || readyFile == "" || entries <= 0 {
		fmt.Fprintln(os.Stderr, "path, ready-file, and positive entries are required")
		os.Exit(2)
	}
	delay, err := time.ParseDuration(delayText)
	if err != nil {
		fmt.Fprintf(os.Stderr, "invalid delay: %v\n", err)
		os.Exit(2)
	}
	compression, err := parseCompression(compressionStr)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	if seal && (sealIntervalUsec == 0 || sealStartUsec == 0) {
		fmt.Fprintln(os.Stderr, "seal interval and start must be positive")
		os.Exit(2)
	}
	return liveConfig{
		path:              path,
		readyFile:         readyFile,
		entries:           entries,
		delay:             delay,
		syncEvery:         syncEvery,
		crashAfter:        crashAfter,
		binaryFixture:     binaryFixture,
		zstdFixture:       zstdFixture,
		xzFixture:         xzFixture,
		lz4Fixture:        lz4Fixture,
		compact:           compact,
		compression:       compression,
		compressThreshold: compressThreshold,
		seal:              seal,
		sealIntervalUsec:  sealIntervalUsec,
		sealStartUsec:     sealStartUsec,
	}
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
		return journal.CompressionNone, fmt.Errorf("unknown compression: %s", value)
	}
}

func releaseLock(lock *journal.WriterLock) {
	if err := lock.Release(); err != nil {
		fmt.Fprintf(os.Stderr, "release writer lock: %v\n", err)
	}
}

func liveOptions(cfg liveConfig) journal.Options {
	opts := journal.Options{
		Compression:            cfg.compression,
		CompressThresholdBytes: cfg.compressThreshold,
		Compact:                cfg.compact,
	}
	if cfg.seal {
		opts.Seal = &journal.SealOptions{
			Seed:         make([]byte, 12),
			IntervalUsec: cfg.sealIntervalUsec,
			StartUsec:    cfg.sealStartUsec,
		}
	}
	return opts
}

func appendLiveEntries(w *journal.Writer, cfg liveConfig) {
	const realtimeBase = uint64(1_700_001_000_000_000)
	for i := 0; i < cfg.entries; i++ {
		fields := liveFields(i, cfg)
		if err := w.Append(fields, journal.EntryOptions{
			RealtimeUsec:  realtimeBase + uint64(i),
			MonotonicUsec: uint64(i + 1),
		}); err != nil {
			_ = w.Close()
			exitWithError(fmt.Sprintf("append %d", i), err)
		}

		syncLiveEntry(w, cfg, i)
		if cfg.crashAfter > 0 && i+1 >= cfg.crashAfter {
			os.Exit(17)
		}
		if cfg.delay > 0 {
			time.Sleep(cfg.delay)
		}
	}

	if err := w.Close(); err != nil {
		exitWithError("close journal", err)
	}
}

func liveFields(index int, cfg liveConfig) []journal.Field {
	if index != 0 {
		return defaultLiveFields(index)
	}
	switch {
	case cfg.binaryFixture:
		return binaryFixtureFields()
	case cfg.zstdFixture:
		return compressedFixtureFields("zstd")
	case cfg.xzFixture:
		return compressedFixtureFields("xz")
	case cfg.lz4Fixture:
		return compressedFixtureFields("lz4")
	default:
		return defaultLiveFields(index)
	}
}

func defaultLiveFields(index int) []journal.Field {
	return []journal.Field{
		journal.StringField("MESSAGE", fmt.Sprintf("live-%06d", index)),
		journal.StringField("PRIORITY", "6"),
		journal.StringField("SYSLOG_IDENTIFIER", "go-live-writer"),
		journal.StringField("LIVE_SEQ", fmt.Sprintf("%06d", index)),
	}
}

func binaryFixtureFields() []journal.Field {
	return []journal.Field{
		journal.StringField("TEST_ID", "binary-interoperability"),
		journal.StringField("MESSAGE", "binary interoperability"),
		journal.StringField("PRIORITY", "6"),
		journal.StringField("LIVE_SEQ", "000000"),
		{Name: "BINARY_PAYLOAD", Value: []byte{0x00, 0x01, 0x02, 'A', '\n', 0x7f, 0x80, 0xff}},
		{Name: "BINARY_MATCH", Value: []byte{'a', 'b', 'c', 0x07, 'd', 'e', 'f'}},
		{Name: "BINARY_EMPTY", Value: []byte{}},
		{Name: "BINARY_COMPRESSIBLE", Value: bytes.Repeat([]byte("A"), 256)},
	}
}

func compressedFixtureFields(name string) []journal.Field {
	largePayload := makeCompressedPayload()
	return []journal.Field{
		journal.StringField("TEST_ID", name+"-interoperability"),
		journal.StringField("MESSAGE", name+" interoperability"),
		journal.StringField("PRIORITY", "6"),
		journal.StringField("LIVE_SEQ", "000000"),
		{Name: "COMPRESSED_PAYLOAD", Value: largePayload},
		{Name: "COMPRESSED_MATCH", Value: largePayload[:32]},
	}
}

func makeCompressedPayload() []byte {
	largePayload := make([]byte, 256)
	for i := range largePayload {
		largePayload[i] = byte(i%26 + 'A')
	}
	return largePayload
}

func syncLiveEntry(w *journal.Writer, cfg liveConfig, index int) {
	if index == 0 {
		if err := w.Sync(); err != nil {
			_ = w.Close()
			exitWithError("sync first entry", err)
		}
		if err := os.WriteFile(cfg.readyFile, []byte("ready\n"), 0o600); err != nil {
			_ = w.Close()
			exitWithError("write ready file", err)
		}
		return
	}
	if cfg.syncEvery > 0 && (index+1)%cfg.syncEvery == 0 {
		if err := w.Sync(); err != nil {
			_ = w.Close()
			exitWithError(fmt.Sprintf("sync %d", index), err)
		}
	}
}

func exitWithError(context string, err error) {
	fmt.Fprintf(os.Stderr, "%s: %v\n", context, err)
	os.Exit(1)
}
